//! 版本化产物只追加到独立目录；原子更新清单，不清理或覆盖业务数据。
use crate::{application::ApplicationIR, project::Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("构建产物 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("构建产物无效：{0}")]
    Invalid(String),
    #[error("构建产物序列化失败：{0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, ArtifactError>;
fn io(path: impl Into<PathBuf>, source: std::io::Error) -> ArtifactError {
    ArtifactError::Io {
        path: path.into(),
        source,
    }
}
fn invalid(message: impl Into<String>) -> ArtifactError {
    ArtifactError::Invalid(message.into())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub framework_version: String,
    pub framework_fingerprint: String,
    pub build_id: String,
    pub input_fingerprint: String,
    pub files: BTreeMap<String, Asset>,
    pub dependencies: BTreeMap<String, String>,
}
impl Manifest {
    fn identity(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(&(
            self.format_version,
            &self.framework_version,
            &self.framework_fingerprint,
            &self.input_fingerprint,
            &self.files,
            &self.dependencies,
        ))?))
    }
}

#[derive(Clone)]
pub struct Loaded {
    pub manifest: Manifest,
    pub application: ApplicationIR,
    pub server_modules: Vec<u8>,
    pub client: PathBuf,
}

/// OS 文件锁随进程退出释放；不会留下阻止恢复的 PID 锁。
pub struct Staging {
    directory: PathBuf,
    root: PathBuf,
    out_dir: PathBuf,
    _lock: fs::File,
}
impl Staging {
    pub fn new(project: &Project) -> Result<Self> {
        let root = project
            .root
            .canonicalize()
            .map_err(|error| io(&project.root, error))?;
        let cache = directory(&root, &root.join(".kenvoy/artifacts"))?;
        let lock_path = cache.join("build.lock");
        reject_link(&lock_path)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io(&lock_path, error))?;
        lock.try_lock()
            .map_err(|_| invalid("另一个构建正在执行，请等待该进程完成"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("系统时间无效"))?
            .as_nanos();
        let stage = cache.join(format!("staging-{}-{stamp}", std::process::id()));
        fs::create_dir(&stage).map_err(|error| io(&stage, error))?;
        fs::create_dir(stage.join("client")).map_err(|error| io(&stage, error))?;
        Ok(Self {
            directory: stage,
            root,
            out_dir: project.out_dir.clone(),
            _lock: lock,
        })
    }
    pub fn client(&self) -> PathBuf {
        self.directory.join("client")
    }
    /// 开发候选先封存版本，只有数据库和运行态发布成功后才激活清单。
    pub fn seal(
        self,
        application: &ApplicationIR,
        server_modules: &[u8],
        input_fingerprint: String,
        dependencies: BTreeMap<String, String>,
    ) -> Result<Loaded> {
        self.finish(application, server_modules, input_fingerprint, dependencies)
    }
    fn finish(
        self,
        application: &ApplicationIR,
        server_modules: &[u8],
        input_fingerprint: String,
        dependencies: BTreeMap<String, String>,
    ) -> Result<Loaded> {
        if !valid_hash(&input_fingerprint) {
            return Err(invalid("输入指纹不是 SHA-256"));
        }
        let mut application = application.clone();
        relocate(&mut application, &self.root)?;
        write_new(
            &self.directory.join("application.ir"),
            &serde_json::to_vec(&application)?,
        )?;
        fs::create_dir(self.directory.join("modules"))
            .map_err(|error| io(&self.directory, error))?;
        write_new(&self.directory.join("modules/server.json"), server_modules)?;
        let files = collect(&self.directory)?;
        if !files.contains_key("client/index.html")
            || !files
                .keys()
                .any(|name| name.starts_with("client/assets/") && name.ends_with(".js"))
        {
            return Err(invalid("客户端产物缺少入口或真实 JavaScript 资源"));
        }
        let mut manifest = Manifest {
            format_version: FORMAT_VERSION,
            framework_version: env!("CARGO_PKG_VERSION").into(),
            framework_fingerprint: crate::frontend::FRAMEWORK_FINGERPRINT.into(),
            build_id: String::new(),
            input_fingerprint,
            files,
            dependencies,
        };
        manifest.build_id = manifest.identity()?;
        let out = directory(&self.root, &self.out_dir)?;
        let owner = out.join(".kenvoy-artifacts");
        reject_link(&owner)?;
        if owner.exists() {
            if fs::read(&owner).map_err(|error| io(&owner, error))? != b"kenvoy-artifacts-v1\n" {
                return Err(invalid("输出目录不属于当前构建格式"));
            }
        } else {
            if fs::read_dir(&out)
                .map_err(|error| io(&out, error))?
                .next()
                .is_some()
            {
                return Err(invalid(
                    "输出目录包含非 Kenvoy 文件，拒绝覆盖；请选择空 out_dir",
                ));
            }
            write_new(&owner, b"kenvoy-artifacts-v1\n")?;
        }
        let versions = directory(&self.root, &out.join("versions"))?;
        let target = versions.join(&manifest.build_id);
        reject_link(&target)?;
        if target.exists() {
            if collect(&target)? != manifest.files {
                return Err(invalid("同指纹版本内容已改变，保留现场并拒绝覆盖"));
            }
        } else {
            fs::rename(&self.directory, &target).map_err(|error| io(&target, error))?;
        }
        load_manifest(&self.root, &out, manifest)
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        // 只删除本次独占创建且仍在原父目录的 staging；已发布版本永不自动清理。
        if self.directory.exists()
            && self.directory.canonicalize().is_ok_and(|path| {
                path == self.directory
                    && path.parent() == Some(self.root.join(".kenvoy/artifacts").as_path())
            })
        {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

pub fn load(project: &Project) -> Result<Loaded> {
    load_paths(&project.root, &project.out_dir)
}
pub fn activate(project: &Project, manifest: &Manifest) -> Result<()> {
    activate_paths(&project.root, &project.out_dir, manifest)
}
fn activate_paths(root: &Path, out: &Path, manifest: &Manifest) -> Result<()> {
    load_manifest(root, out, manifest.clone())?;
    let encoded = serde_json::to_vec_pretty(manifest)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("系统时间无效"))?
        .as_nanos();
    let temp = out.join(format!(".manifest-{}-{stamp}.tmp", std::process::id()));
    write_new(&temp, &encoded)?;
    let destination = out.join("manifest.json");
    reject_link(&destination)?;
    fs::rename(&temp, &destination).map_err(|error| io(&destination, error))
}
fn load_paths(root: &Path, out: &Path) -> Result<Loaded> {
    check_directory(root, out)?;
    let manifest_path = out.join("manifest.json");
    let bytes = read_limited(&manifest_path, 4 * 1024 * 1024)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    load_manifest(root, out, manifest)
}
fn load_manifest(root: &Path, out: &Path, manifest: Manifest) -> Result<Loaded> {
    check_directory(root, out)?;
    if manifest.format_version != FORMAT_VERSION
        || manifest.framework_version != env!("CARGO_PKG_VERSION")
        || manifest.framework_fingerprint != crate::frontend::FRAMEWORK_FINGERPRINT
        || !valid_hash(&manifest.build_id)
        || !valid_hash(&manifest.input_fingerprint)
        || manifest.identity()? != manifest.build_id
    {
        return Err(invalid("清单版本、框架版本或内容标识不匹配，请重新构建"));
    }
    let version = out.join("versions").join(&manifest.build_id);
    check_directory(root, &version)?;
    if manifest.files != collect(&version)? {
        return Err(invalid("产物文件清单、大小或 SHA-256 不匹配"));
    }
    for required in ["application.ir", "modules/server.json", "client/index.html"] {
        if !manifest.files.contains_key(required) {
            return Err(invalid(format!("缺少产物 {required}")));
        }
    }
    let application: ApplicationIR =
        serde_json::from_slice(&read_limited(&version.join("application.ir"), MAX_FILE)?)?;
    let server_modules = read_limited(&version.join("modules/server.json"), MAX_FILE)?;
    Ok(Loaded {
        manifest,
        application,
        server_modules,
        client: version.join("client"),
    })
}

fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid(format!("产物路径不能是链接：{}", path.display())))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}
fn check_directory(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| invalid("产物路径越过项目根"))?;
    let mut cursor = root.to_owned();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(invalid("产物路径包含非法组件"));
        }
        cursor.push(component);
        reject_link(&cursor)?;
    }
    let resolved = path.canonicalize().map_err(|error| io(path, error))?;
    if !resolved.starts_with(root) || !resolved.is_dir() {
        return Err(invalid("产物目录越界或不是目录"));
    }
    Ok(())
}
fn directory(root: &Path, path: &Path) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| invalid("产物目录越界"))?;
    let mut cursor = root.to_owned();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(invalid("产物目录包含非法组件"));
        }
        cursor.push(component);
        reject_link(&cursor)?;
        if !cursor.exists() {
            fs::create_dir(&cursor).map_err(|error| io(&cursor, error))?;
        }
        check_directory(root, &cursor)?;
    }
    Ok(cursor)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(invalid("单个产物超过 32 MiB 限制"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io(path, error))?;
    file.write_all(bytes).map_err(|error| io(path, error))?;
    file.sync_all().map_err(|error| io(path, error))
}
fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    reject_link(path)?;
    let metadata = fs::metadata(path).map_err(|error| io(path, error))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid(format!(
            "文件不是普通文件或超过大小限制：{}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| io(path, error))?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(invalid("读取过程中产物超过大小限制"));
    }
    Ok(bytes)
}
fn collect(root: &Path) -> Result<BTreeMap<String, Asset>> {
    fn walk(
        root: &Path,
        folder: &Path,
        files: &mut BTreeMap<String, Asset>,
        aliases: &mut BTreeSet<String>,
        total: &mut u64,
    ) -> Result<()> {
        for entry in fs::read_dir(folder).map_err(|error| io(folder, error))? {
            let path = entry.map_err(|error| io(folder, error))?.path();
            reject_link(&path)?;
            if path.is_dir() {
                walk(root, &path, files, aliases, total)?;
                continue;
            }
            let name = path
                .strip_prefix(root)
                .map_err(|_| invalid("产物路径越界"))?
                .to_str()
                .ok_or_else(|| invalid("产物路径必须为 UTF-8"))?
                .replace('\\', "/");
            if name != "application.ir"
                && name != "modules/server.json"
                && !name.starts_with("client/")
            {
                return Err(invalid(format!("未知产物文件：{name}")));
            }
            if !aliases.insert(name.to_lowercase()) || files.len() >= 10000 {
                return Err(invalid("产物路径大小写冲突或文件数量超过 10000"));
            }
            let bytes = read_limited(&path, MAX_FILE)?;
            *total += bytes.len() as u64;
            if *total > MAX_TOTAL {
                return Err(invalid("产物总大小超过 256 MiB"));
            }
            files.insert(
                name,
                Asset {
                    size: bytes.len() as u64,
                    sha256: digest(&bytes),
                },
            );
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files, &mut BTreeSet::new(), &mut 0)?;
    Ok(files)
}

/// 比较源文件、CSS/静态资产和锁文件；不读取数据目录、环境密钥或构建缓存。
pub fn input_fingerprint(project: &Project) -> Result<String> {
    fn walk(
        project: &Project,
        folder: &Path,
        files: &mut BTreeMap<String, String>,
        total: &mut u64,
    ) -> Result<()> {
        for entry in fs::read_dir(folder).map_err(|error| io(folder, error))? {
            let entry = entry.map_err(|error| io(folder, error))?;
            let path = entry.path();
            if path == project.out_dir || project.database.parent() == Some(path.as_path()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(
                name.as_str(),
                ".git" | ".kenvoy" | "node_modules" | "target"
            ) || name.starts_with(".env")
            {
                continue;
            }
            reject_link(&path)?;
            if path.is_dir() {
                walk(project, &path, files, total)?;
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !matches!(
                extension,
                "ts" | "tsx"
                    | "mts"
                    | "cts"
                    | "js"
                    | "jsx"
                    | "mjs"
                    | "cjs"
                    | "css"
                    | "json"
                    | "html"
                    | "toml"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "avif"
                    | "ico"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "otf"
            ) {
                continue;
            }
            if files.len() >= 10000 {
                return Err(invalid("源码输入超过 10000 个文件"));
            }
            let bytes = read_limited(&path, MAX_FILE)?;
            *total += bytes.len() as u64;
            if *total > MAX_TOTAL {
                return Err(invalid("源码输入总大小超过 256 MiB"));
            }
            let relative = path
                .strip_prefix(&project.root)
                .map_err(|_| invalid("源码输入越界"))?
                .to_str()
                .ok_or_else(|| invalid("源码路径必须为 UTF-8"))?
                .replace('\\', "/");
            files.insert(relative, digest(&bytes));
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(project, &project.root, &mut files, &mut 0)?;
    Ok(digest(&serde_json::to_vec(&(
        env!("CARGO_PKG_VERSION"),
        crate::frontend::FRAMEWORK_FINGERPRINT,
        crate::frontend::fingerprint(),
        files,
    ))?))
}

fn relocate(application: &mut ApplicationIR, root: &Path) -> Result<()> {
    use crate::application::{ComponentDefinition as C, NavigationItem as N};
    fn source(source: &mut crate::dsl::SourceLocation, root: &Path) -> Result<()> {
        source.path = source
            .path
            .strip_prefix(root)
            .map_err(|_| invalid("应用源码位置越过项目根"))?
            .to_owned();
        Ok(())
    }
    fn components(nodes: &mut [C], root: &Path) -> Result<()> {
        for node in nodes {
            match node {
                C::Form { source: s, .. }
                | C::Table { source: s, .. }
                | C::Text { source: s, .. } => source(s, root)?,
                C::Group {
                    source: s,
                    children,
                    ..
                }
                | C::Custom {
                    source: s,
                    children,
                    ..
                } => {
                    source(s, root)?;
                    components(children, root)?;
                }
            }
        }
        Ok(())
    }
    fn navigation(nodes: &mut [N], root: &Path) -> Result<()> {
        for node in nodes {
            match node {
                N::Link { source: s, .. } => source(s, root)?,
                N::Group {
                    source: s, items, ..
                } => {
                    source(s, root)?;
                    navigation(items, root)?;
                }
            }
        }
        Ok(())
    }
    for resource in &mut application.resources {
        source(&mut resource.source, root)?;
        for field in &mut resource.fields {
            source(&mut field.source, root)?;
        }
    }
    for page in &mut application.pages {
        source(&mut page.source, root)?;
        components(&mut page.components, root)?;
    }
    for action in &mut application.actions {
        source(&mut action.source, root)?;
    }
    for nav in &mut application.navigation {
        source(&mut nav.source, root)?;
        navigation(&mut nav.items, root)?;
    }
    for reference in &mut application.module_references {
        source(&mut reference.source, root)?;
        if !reference.external {
            reference.module = reference
                .module
                .strip_prefix(root)
                .map_err(|_| invalid("模块位置越过项目根"))?
                .to_owned();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_versions_detect_tampering_and_preserve_data()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-artifact-{}-{stamp}", std::process::id()));
        crate::project::create(&path)?;
        let project = Project::load(&path)?;
        let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
            let (application, _) = crate::compile(&project, &mut kengine::AnalysisSession::new())?;
            let before = input_fingerprint(&project)?;
            fs::write(&project.database, "用户数据保护夹具".as_bytes())?;
            assert_eq!(before, input_fingerprint(&project)?);
            let stage = Staging::new(&project)?;
            assert!(Staging::new(&project).is_err());
            crate::frontend::export(&stage.client())?;
            let loaded = stage.seal(&application, b"{}", before.clone(), BTreeMap::new())?;
            assert!(!project.out_dir.join("manifest.json").exists());
            activate(&project, &loaded.manifest)?;
            assert!(loaded.client.join("index.html").is_file());
            assert_eq!(load(&project)?.manifest, loaded.manifest);
            assert_eq!(loaded.server_modules, b"{}");
            assert!(!loaded.application.resources[0].source.path.is_absolute());
            assert_eq!(before, input_fingerprint(&project)?);
            let stage = Staging::new(&project)?;
            crate::frontend::export(&stage.client())?;
            assert_eq!(
                stage
                    .seal(&application, b"{}", before, BTreeMap::new())?
                    .manifest,
                loaded.manifest
            );
            fs::write(loaded.client.join("index.html"), "用户修改")?;
            assert!(load(&project).is_err());
            assert_eq!(fs::read(&project.database)?, "用户数据保护夹具".as_bytes());
            Ok(())
        })();
        if path.parent() == Some(std::env::temp_dir().as_path()) {
            fs::remove_dir_all(&path)?;
        }
        result
    }

    #[test]
    fn failed_build_keeps_unmanaged_output_and_source_changes_change_fingerprint()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-artifact-unmanaged-{}-{stamp}",
            std::process::id()
        ));
        crate::project::create(&path)?;
        let project = Project::load(&path)?;
        let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
            let (application, _) = crate::compile(&project, &mut kengine::AnalysisSession::new())?;
            let initial = input_fingerprint(&project)?;
            fs::write(project.root.join(".env"), "SECRET=not-for-client")?;
            assert_eq!(initial, input_fingerprint(&project)?);
            fs::write(project.root.join("src/theme.css"), "body { color: red; }")?;
            assert_ne!(initial, input_fingerprint(&project)?);
            for extension in ["mts", "cts", "avif", "ico", "ttf", "otf"] {
                let asset = project.root.join(format!("src/input.{extension}"));
                fs::write(&asset, "变更前")?;
                let previous = input_fingerprint(&project)?;
                fs::write(&asset, "变更后")?;
                assert_ne!(
                    previous,
                    input_fingerprint(&project)?,
                    "{extension} 变更必须使产物失效"
                );
            }
            fs::create_dir(&project.out_dir)?;
            fs::write(project.out_dir.join("user.txt"), "保留已有输出")?;
            let stage = Staging::new(&project)?;
            crate::frontend::export(&stage.client())?;
            assert!(
                stage
                    .seal(&application, b"{}", initial, BTreeMap::new())
                    .is_err()
            );
            assert_eq!(
                fs::read_to_string(project.out_dir.join("user.txt"))?,
                "保留已有输出"
            );
            assert!(!project.out_dir.join("manifest.json").exists());
            assert!(!project.database.exists());
            Ok(())
        })();
        if path.parent() == Some(std::env::temp_dir().as_path()) {
            fs::remove_dir_all(&path)?;
        }
        result
    }
}
