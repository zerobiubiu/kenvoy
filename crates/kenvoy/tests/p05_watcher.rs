//! 阶段 00 原生文件监听与候选发布探针；这里的局部快照不是正式 Generic/Application IR。
//! 不涉及真实数据库迁移、HTTP 更新、React HMR 或完整 TypeScript 模块解析。

use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    error::Error,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use notify::{Event, EventKind, RecursiveMode, Watcher};
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_ast::{ModuleDecl, ModuleItem};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

type ProbeResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
type Graph = BTreeMap<PathBuf, BTreeSet<PathBuf>>;

struct ScratchDirectory(PathBuf);
impl ScratchDirectory {
    fn new() -> ProbeResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("kenvoy-p05-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(fs::canonicalize(path)?))
    }
}
impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        if let Ok(parent) = fs::canonicalize(std::env::temp_dir())
            && self.0.parent() == Some(parent.as_path())
            && self
                .0
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("kenvoy-p05-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn fingerprint(source: &str) -> u64 {
    // 仅作进程内探针缓存键；原文还需比较，不把非加密 hash 当完整性校验。
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn parse_imports(path: &Path, source: &str) -> Result<BTreeSet<PathBuf>, String> {
    let map: Lrc<SourceMap> = Default::default();
    let file = map.new_source_file(FileName::Real(path.to_owned()).into(), source.to_owned());
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: path.extension().is_some_and(|extension| extension == "tsx"),
        ..Default::default()
    });
    let mut parser = Parser::new_from(Lexer::new(
        syntax,
        Default::default(),
        StringInput::from(&*file),
        None,
    ));
    let module = parser.parse_module().map_err(|error| format!("{error:?}"));
    let recovered = parser.take_errors();
    if !recovered.is_empty() {
        return Err(format!("可恢复语法错误：{recovered:?}"));
    }
    let mut imports = BTreeSet::new();
    for item in module?.body {
        if let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item {
            let specifier = import.src.value.to_string_lossy();
            if !specifier.starts_with("./") {
                return Err("本探针仅支持当前目录的相对导入".into());
            }
            let parent = path.parent().ok_or("缺少模块父目录")?;
            let candidate = parent.join(specifier.as_ref());
            let resolved = [
                candidate.clone(),
                candidate.with_extension("ts"),
                candidate.with_extension("tsx"),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .ok_or("导入模块不存在")?;
            imports.insert(fs::canonicalize(resolved).map_err(|error| error.to_string())?);
        }
    }
    Ok(imports)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    generation: u64,
    graph: Graph,
    source_hashes: BTreeMap<PathBuf, u64>,
}
struct CachedModule {
    source: String,
    hash: u64,
    imports: Result<BTreeSet<PathBuf>, String>,
}
struct CandidateProbe {
    cache: BTreeMap<PathBuf, CachedModule>,
    assets: BTreeMap<PathBuf, String>,
    parsed: BTreeMap<PathBuf, usize>,
    snapshot: Snapshot,
    reanalyzed: BTreeSet<PathBuf>,
    diagnostic: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
enum Update {
    Published,
    Noop,
    Rejected,
}

impl CandidateProbe {
    fn new() -> Self {
        Self {
            cache: BTreeMap::new(),
            assets: BTreeMap::new(),
            parsed: BTreeMap::new(),
            snapshot: Snapshot {
                generation: 0,
                graph: BTreeMap::new(),
                source_hashes: BTreeMap::new(),
            },
            reanalyzed: BTreeSet::new(),
            diagnostic: None,
        }
    }

    fn update(&mut self, paths: BTreeSet<PathBuf>) -> ProbeResult<Update> {
        let mut changed = BTreeSet::new();
        let mut asset_changed = false;
        let mut configuration_changed = false;
        for path in paths.into_iter().filter(|path| path.is_file()) {
            let source = fs::read_to_string(&path)?;
            let is_source = path
                .extension()
                .is_some_and(|extension| extension == "tsx" || extension == "ts");
            if is_source {
                let hash = fingerprint(&source);
                if self
                    .cache
                    .get(&path)
                    .is_some_and(|cached| cached.hash == hash && cached.source == source)
                {
                    continue;
                }
                *self.parsed.entry(path.clone()).or_default() += 1;
                let imports = parse_imports(&path, &source);
                self.cache.insert(
                    path.clone(),
                    CachedModule {
                        source,
                        hash,
                        imports,
                    },
                );
                changed.insert(path);
            } else if path.extension().is_some_and(|extension| {
                extension == "css" || extension == "toml" || extension == "txt"
            }) {
                if self.assets.get(&path) == Some(&source) {
                    continue;
                }
                configuration_changed |= path
                    .extension()
                    .is_some_and(|extension| extension == "toml");
                self.assets.insert(path, source);
                asset_changed = true;
            }
        }
        if changed.is_empty() && !asset_changed {
            return Ok(Update::Noop);
        }
        let mut graph = BTreeMap::new();
        for (path, cached) in &self.cache {
            match &cached.imports {
                Ok(imports) => {
                    graph.insert(path.clone(), imports.clone());
                }
                Err(error) => {
                    self.diagnostic = Some(format!("{}：{error}", path.display()));
                    return Ok(Update::Rejected);
                }
            }
        }
        let mut affected = changed;
        if configuration_changed {
            affected.extend(graph.keys().cloned());
        }
        loop {
            let previous = affected.len();
            for (module, imports) in graph.iter().chain(self.snapshot.graph.iter()) {
                if !imports.is_disjoint(&affected) {
                    affected.insert(module.clone());
                }
            }
            if affected.len() == previous {
                break;
            }
        }
        self.snapshot = Snapshot {
            generation: self.snapshot.generation + 1,
            graph,
            source_hashes: self
                .cache
                .iter()
                .map(|(path, cached)| (path.clone(), cached.hash))
                .collect(),
        };
        self.reanalyzed = affected;
        self.diagnostic = None;
        Ok(Update::Published)
    }
}

fn observed_paths(
    receiver: &Receiver<notify::Result<Event>>,
    target: &Path,
) -> ProbeResult<BTreeSet<PathBuf>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut paths = BTreeSet::new();
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "P05 等待目标文件事件超过5秒")
            })?;
        let event = receiver.recv_timeout(remaining)??;
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            continue;
        }
        let found = event.paths.iter().any(|path| path == target);
        paths.extend(event.paths);
        if found {
            return Ok(paths);
        }
    }
}

#[test]
fn native_watcher_incremental_candidates_preserve_last_valid_snapshot() -> ProbeResult {
    let scratch = ScratchDirectory::new()?;
    let root = &scratch.0;
    let app = root.join("app.tsx");
    let fields = root.join("Fields.tsx");
    let shared = root.join("shared.ts");
    let unrelated = root.join("unrelated.ts");
    let css = root.join("style.css");
    let config = root.join("kenvoy.toml");
    let public = root.join("public");
    fs::create_dir(&public)?;
    let public_file = public.join("hello.txt");
    fs::write(
        &app,
        "import Fields from './Fields'; export default <app><Fields /></app>;",
    )?;
    fs::write(
        &fields,
        "import { caption } from './shared'; export default <text label={caption} />;",
    )?;
    fs::write(&shared, "export const caption: string = '姓名';")?;
    fs::write(&unrelated, "export const independent = 1;")?;
    fs::write(&css, "body { color: black; }")?;
    fs::write(&config, "[project]\nname = 'probe'\n")?;
    fs::write(&public_file, "hello")?;
    let mut probe = CandidateProbe::new();
    let initial = [
        &app,
        &fields,
        &shared,
        &unrelated,
        &css,
        &config,
        &public_file,
    ]
    .into_iter()
    .cloned()
    .collect();
    assert_eq!(probe.update(initial)?, Update::Published);
    assert_eq!(probe.snapshot.generation, 1);
    assert_eq!(probe.parsed.values().sum::<usize>(), 4);
    assert_eq!(probe.snapshot.graph[&app], BTreeSet::from([fields.clone()]));
    assert_eq!(
        probe.snapshot.graph[&fields],
        BTreeSet::from([shared.clone()])
    );
    let (sender, receiver) = mpsc::sync_channel(128);
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.try_send(event);
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;

    fs::write(&shared, "export const caption: string = '客户姓名';")?;
    assert_eq!(
        probe.update(observed_paths(&receiver, &shared)?)?,
        Update::Published
    );
    assert_eq!(probe.parsed[&shared], 2);
    assert_eq!(probe.parsed[&app], 1);
    assert_eq!(probe.parsed[&fields], 1);
    assert_eq!(probe.parsed[&unrelated], 1);
    assert_eq!(
        probe.reanalyzed,
        BTreeSet::from([app.clone(), fields.clone(), shared.clone()])
    );

    let replacement = root.join("app.tmp");
    fs::write(
        &replacement,
        "import Fields from './Fields'; export default <app name='更新'><Fields /></app>;",
    )?;
    fs::rename(&replacement, &app)?;
    assert_eq!(
        probe.update(observed_paths(&receiver, &app)?)?,
        Update::Published
    );
    assert_eq!(probe.parsed[&app], 2);
    assert_eq!(probe.reanalyzed, BTreeSet::from([app.clone()]));
    let before_noop = probe.snapshot.clone();
    let parsed_before_noop = probe.parsed.clone();
    fs::write(&unrelated, "export const independent = 1;")?;
    assert_eq!(
        probe.update(observed_paths(&receiver, &unrelated)?)?,
        Update::Noop
    );
    assert_eq!(probe.snapshot, before_noop);
    assert_eq!(probe.parsed, parsed_before_noop);

    fs::write(&fields, "export default <text label={;")?;
    assert_eq!(
        probe.update(observed_paths(&receiver, &fields)?)?,
        Update::Rejected
    );
    assert!(
        probe
            .diagnostic
            .as_ref()
            .is_some_and(|diagnostic| diagnostic.contains("Fields.tsx"))
    );
    assert_eq!(probe.snapshot, before_noop);
    assert_eq!(
        probe.update(BTreeSet::from([fields.clone()]))?,
        Update::Noop
    );
    assert_eq!(probe.parsed[&fields], 2);
    fs::write(
        &fields,
        "import { caption } from './shared'; export default <text label={caption} required />;",
    )?;
    assert_eq!(
        probe.update(observed_paths(&receiver, &fields)?)?,
        Update::Published
    );
    assert_eq!(probe.snapshot.generation, before_noop.generation + 1);
    assert_eq!(probe.parsed[&fields], 3);
    assert!(probe.diagnostic.is_none());
    let parsed_before_assets = probe.parsed.clone();
    for (path, source) in [
        (&css, "body { color: blue; }"),
        (&config, "[project]\nname = 'updated'\n"),
        (&public_file, "updated"),
    ] {
        let generation = probe.snapshot.generation;
        fs::write(path, source)?;
        assert_eq!(
            probe.update(observed_paths(&receiver, path)?)?,
            Update::Published
        );
        assert_eq!(probe.snapshot.generation, generation + 1);
        assert_eq!(probe.assets[path], source);
        assert_eq!(probe.parsed, parsed_before_assets);
    }
    println!(
        "P05 {:?}：4模块；普通保存/原子替换；hash no-op；反向依赖；错误保留generation并修复；CSS/config/public均通过，最终generation={}",
        notify::RecommendedWatcher::kind(),
        probe.snapshot.generation
    );
    watcher.unwatch(root)?;
    drop(watcher);
    drop(receiver);
    let root = root.clone();
    drop(scratch);
    assert!(!root.exists(), "watcher 关闭后应释放并清理整个临时目录");
    Ok(())
}
