//! 自定义客户端只在独占 staging 中构建；框架缓存与用户依赖安装分开管理。
#[path = "client_development.rs"]
pub mod development;
#[cfg(test)]
#[path = "client_build_tests.rs"]
mod tests;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kengine::{DependencyKind, ProgramIR, ResolvedModule};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    application::{ApplicationIR, ModuleReferenceKind},
    javascript::{
        JsDiagnostic,
        source_slice::{ClientImportTarget, client_slice},
    },
    project::Project,
};

const PACKAGE: &[u8] = include_bytes!("../frontend/package.json");
const LOCK: &[u8] = include_bytes!("../frontend/package-lock.json");
const SOURCES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../frontend/index.html")),
    ("src/main.tsx", include_bytes!("../frontend/src/main.tsx")),
    ("src/App.tsx", include_bytes!("../frontend/src/App.tsx")),
    (
        "src/Runtime.tsx",
        include_bytes!("../frontend/src/Runtime.tsx"),
    ),
    ("src/Admin.tsx", include_bytes!("../frontend/src/Admin.tsx")),
    ("src/theme.ts", include_bytes!("../frontend/src/theme.ts")),
    (
        "src/styles.css",
        include_bytes!("../frontend/src/styles.css"),
    ),
    ("src/types.ts", include_bytes!("../frontend/src/types.ts")),
    (
        "src/validation.ts",
        include_bytes!("../frontend/src/validation.ts"),
    ),
    ("src/api.ts", include_bytes!("../frontend/src/api.ts")),
    ("src/sdk.ts", include_bytes!("../frontend/src/sdk.ts")),
    ("src/ui.tsx", include_bytes!("../frontend/src/ui.tsx")),
    ("src/files.ts", include_bytes!("../frontend/src/files.ts")),
    (
        "src/uploads.ts",
        include_bytes!("../frontend/src/uploads.ts"),
    ),
    (
        "src/images.tsx",
        include_bytes!("../frontend/src/images.tsx"),
    ),
    (
        "src/FileField.tsx",
        include_bytes!("../frontend/src/FileField.tsx"),
    ),
    (
        "src/bindings.ts",
        include_bytes!("../frontend/src/bindings.ts"),
    ),
    (
        "src/options.ts",
        include_bytes!("../frontend/src/options.ts"),
    ),
    (
        "src/context.ts",
        include_bytes!("../frontend/src/context.ts"),
    ),
    ("src/env.d.ts", include_bytes!("../frontend/src/env.d.ts")),
    (
        "client-builder.mjs",
        include_bytes!("../frontend/scripts/client-builder.mjs"),
    ),
    (
        "client-dev.mjs",
        include_bytes!("../frontend/scripts/client-dev.mjs"),
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum ClientBuildError {
    #[error("客户端构建文件 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("客户端构建边界无效：{0}")]
    Invalid(String),
    #[error("客户端构建数据无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Slice(#[from] JsDiagnostic),
    #[error(transparent)]
    Frontend(#[from] crate::frontend::FrontendError),
    #[error(transparent)]
    Artifact(#[from] crate::artifact::ArtifactError),
    #[error("{operation} 进程启动失败：{source}")]
    Spawn {
        operation: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation} 超过 {seconds} 秒限制，进程已停止")]
    Timeout { operation: String, seconds: u64 },
    #[error("{operation} 失败（退出码 {code:?}）：{output}")]
    Process {
        operation: String,
        code: Option<i32>,
        output: String,
    },
}
pub type Result<T> = std::result::Result<T, ClientBuildError>;
fn io(path: impl Into<PathBuf>, source: std::io::Error) -> ClientBuildError {
    ClientBuildError::Io {
        path: path.into(),
        source,
    }
}
fn invalid(message: impl Into<String>) -> ClientBuildError {
    ClientBuildError::Invalid(message.into())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClientBuildOptions {
    pub install_deps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientBuildOutput {
    pub client_dir: PathBuf,
    pub fingerprint: String,
    pub files: BTreeMap<String, ClientAsset>,
    pub dependencies: BTreeMap<String, String>,
    #[serde(skip)]
    pub(crate) seed: Option<DevSeed>,
}
#[derive(Debug, Clone)]
pub(crate) struct DevSeed {
    node: PathBuf,
    request: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAsset {
    pub size: u64,
    pub sha256: String,
}

/// 无自定义模块时不需要 Node；自定义构建不能覆盖任何既有产物。
pub async fn build(
    project: &Project,
    program: &ProgramIR,
    app: &ApplicationIR,
    destination: &Path,
    options: ClientBuildOptions,
) -> Result<ClientBuildOutput> {
    let root = project
        .root
        .canonicalize()
        .map_err(|error| io(&project.root, error))?;
    let destination = destination
        .canonicalize()
        .map_err(|error| io(destination, error))?;
    if !destination.starts_with(root.join(".kenvoy"))
        || fs::read_dir(&destination)
            .map_err(|error| io(&destination, error))?
            .next()
            .is_some()
    {
        return Err(invalid(
            "输出必须是项目 .kenvoy 内独占创建的空 staging 目录",
        ));
    }
    let references: Vec<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind != ModuleReferenceKind::ServerFunction)
        .collect();
    if references.is_empty() {
        crate::frontend::export(&destination)?;
        return output(destination, BTreeMap::new());
    }
    if program.has_errors() {
        return Err(invalid("不能构建有语言诊断的候选模块图"));
    }
    verify_sources(program, &root)?;
    let cache = directory(&root, &root.join(".kenvoy/client-toolchain"))?;
    let lock_path = cache.join("build.lock");
    reject_link(&lock_path)?;
    let guard = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| io(&lock_path, error))?;
    guard
        .try_lock()
        .map_err(|_| invalid("另一个客户端构建正在执行"))?;
    let node_info = run("node", &[OsString::from("-e"), OsString::from("console.log(JSON.stringify({executable:process.execPath,version:process.versions.node,platform:process.platform,architecture:process.arch}))")], &root, "Node 运行环境检查", 10).await?;
    let node: NodeInfo = serde_json::from_str(node_info.trim())?;
    if node
        .version
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .is_none_or(|version| version < 24)
    {
        return Err(invalid("自定义客户端构建需要 Node.js 24 或更新版本"));
    }
    let toolchain = directory(
        &root,
        &cache.join(format!(
            "{}-{}-{}-node{}",
            &digest(LOCK)[..24],
            node.platform,
            node.architecture,
            node.version.split('.').next().unwrap_or("unknown")
        )),
    )?;
    immutable(&toolchain.join("package.json"), PACKAGE)?;
    immutable(&toolchain.join("package-lock.json"), LOCK)?;
    let mut source_hash = Sha256::new();
    for (name, bytes) in SOURCES {
        source_hash.update(name.as_bytes());
        source_hash.update(bytes);
    }
    let source_hash: String = source_hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let framework = directory(&root, &toolchain.join("sources").join(source_hash))?;
    for (name, bytes) in SOURCES {
        let path = framework.join(name);
        if let Some(parent) = path.parent() {
            directory(&root, parent)?;
        }
        immutable(&path, bytes)?;
    }
    // 框架缓存仅安装锁文件中的确定依赖，不读取项目 npmrc 或生命周期脚本。
    let ready = toolchain.join("ready.json");
    let marker = json!({"lock":digest(LOCK),"node_major":node.version.split('.').next(),"platform":node.platform,"architecture":node.architecture});
    if !ready.exists() {
        tracing::info!(target: "kenvoy::build", "首次准备锁定的客户端构建工具缓存");
        let npm = npm_cli(&node.executable)?;
        npm_command(
            &node.executable,
            &npm,
            &toolchain,
            &[
                "ci",
                "--ignore-scripts",
                "--include=dev",
                "--no-audit",
                "--no-fund",
                "--userconfig",
                "/dev/null",
            ],
            cfg!(windows),
        )
        .await?;
        immutable(&ready, &serde_json::to_vec(&marker)?)?;
    } else if fs::read(&ready).map_err(|error| io(&ready, error))? != serde_json::to_vec(&marker)? {
        return Err(invalid(
            "客户端工具缓存与 Node 主版本不一致；请保留缓存并换用匹配运行环境",
        ));
    }
    run(
        &node.executable,
        &[
            framework.join("client-builder.mjs").into_os_string(),
            "--probe".into(),
        ],
        &toolchain,
        "客户端工具缓存验证",
        20,
    )
    .await?;
    if options.install_deps {
        install_project_dependencies(project, &node.executable).await?;
    }
    let packages = project
        .external_packages()
        .map_err(|error| invalid(error.to_string()))?;
    let private = private_modules(program, app);
    let mut items = vec![];
    for reference in references {
        let module = if reference.external {
            reference.module.clone()
        } else {
            reference
                .module
                .canonicalize()
                .map_err(|error| io(&reference.module, error))?
        };
        let kind = match reference.kind {
            ModuleReferenceKind::ClientComponent => "component",
            ModuleReferenceKind::ClientExpression => "expression",
            _ => "function",
        };
        let mut item =
            json!({"id":reference.id,"kind":kind,"module":module,"export_name":reference.export});
        if reference.kind == ModuleReferenceKind::ClientExpression
            || (!reference.external
                && (module == project.entry || module.to_string_lossy().contains(".diu.")))
        {
            let slice = client_slice(program, app, reference)?;
            let imports: BTreeMap<_, _> = slice
                .imports
                .into_iter()
                .map(|(name, target)| {
                    (
                        name,
                        match target {
                            ClientImportTarget::File(path) => json!({"kind":"file","module":path}),
                            ClientImportTarget::External(name) => {
                                json!({"kind":"external","module":name})
                            }
                        },
                    )
                })
                .collect();
            item["slice"] = json!({"code":slice.code,"export_name":slice.export_name,"imports":imports,"source":slice.source});
        }
        items.push(item);
    }
    let request_dir = directory(&root, &cache.join("requests"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("系统时间无效"))?
        .as_nanos();
    let report = request_dir.join(format!("{}-{stamp}-report.json", std::process::id()));
    let request_path = request_dir.join(format!("{}-{stamp}-request.json", std::process::id()));
    let request = json!({"version":1,"project_root":root,"framework_root":framework,"toolchain_root":toolchain,"destination":destination,
        "private_modules":private,"declaration_modules":[project.entry],"external_packages":packages,
        "source_modules":program.modules.iter().map(|module| (&module.path,digest(module.source.as_bytes()))).collect::<BTreeMap<_,_>>(),
        "references":items,"report":report});
    immutable(&request_path, &serde_json::to_vec(&request)?)?;
    let result = run(
        &node.executable,
        &[
            framework.join("client-builder.mjs").into_os_string(),
            request_path.clone().into_os_string(),
        ],
        &toolchain,
        "客户端资源构建",
        120,
    )
    .await;
    // 仅删除本次 create_new 创建的参数文件，既有缓存和失败产物保留用于排查。
    fs::remove_file(&request_path).map_err(|error| io(&request_path, error))?;
    result?;
    verify_sources(program, &root)?;
    let data: BuilderReport =
        serde_json::from_slice(&fs::read(&report).map_err(|error| io(&report, error))?)?;
    fs::remove_file(&report).map_err(|error| io(&report, error))?;
    if data.version != 1 {
        return Err(invalid("构建报告版本不受支持"));
    }
    let mut output = output(
        destination,
        data.dependencies
            .into_iter()
            .map(|dependency| {
                (
                    format!("{}:{}", dependency.scope, dependency.name),
                    dependency.version,
                )
            })
            .collect(),
    )?;
    output.seed = Some(DevSeed {
        node: node.executable,
        request,
    });
    Ok(output)
}

#[derive(Deserialize)]
struct NodeInfo {
    executable: PathBuf,
    version: String,
    platform: String,
    architecture: String,
}
#[derive(Deserialize)]
struct BuilderReport {
    version: u32,
    dependencies: Vec<DependencyReport>,
}
#[derive(Deserialize)]
struct DependencyReport {
    name: String,
    version: String,
    scope: String,
}

fn verify_sources(program: &ProgramIR, root: &Path) -> Result<()> {
    for module in &program.modules {
        let path = module
            .path
            .canonicalize()
            .map_err(|error| io(&module.path, error))?;
        if !path.starts_with(root) {
            return Err(invalid("分析模块越过项目根目录"));
        }
        if fs::read(&path).map_err(|error| io(&path, error))? != module.source.as_bytes() {
            return Err(invalid("分析后源码已改变，请重新分析再构建"));
        }
    }
    Ok(())
}

fn private_modules(program: &ProgramIR, app: &ApplicationIR) -> BTreeSet<PathBuf> {
    let shared: BTreeSet<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::SharedFunction)
        .map(|reference| &reference.module)
        .collect();
    let server_roots: BTreeSet<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
        .map(|reference| reference.module.clone())
        .collect();
    let mut queue: VecDeque<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
        .map(|reference| reference.module.clone())
        .chain(
            program
                .modules
                .iter()
                .filter(|module| module.path.to_string_lossy().contains(".server."))
                .map(|module| module.path.clone()),
        )
        .collect();
    let mut result = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        let name = path.to_string_lossy();
        if !server_roots.contains(&path)
            && !name.contains(".server.")
            && (shared.contains(&path) || name.contains(".shared."))
        {
            continue;
        }
        if !result.insert(path.clone()) {
            continue;
        }
        for edge in program.module_graph.edges.iter().filter(|edge| {
            edge.importer == path
                && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
        }) {
            if let Some(ResolvedModule::File { path }) = &edge.target {
                queue.push_back(path.clone());
            }
        }
    }
    result
}

fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid("构建缓存不能通过符号链接替换"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}
fn directory(root: &Path, path: &Path) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| invalid("构建缓存越过项目根目录"))?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(invalid("构建缓存路径无效"));
        };
        current.push(part);
        reject_link(&current)?;
        if !current.exists() {
            fs::create_dir(&current).map_err(|error| io(&current, error))?;
        }
        current = current
            .canonicalize()
            .map_err(|error| io(&current, error))?;
        if !current.starts_with(root) || !current.is_dir() {
            return Err(invalid("构建缓存链接越界或不是目录"));
        }
    }
    Ok(current)
}
fn immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_link(path)?;
    if path.exists() {
        if fs::read(path).map_err(|error| io(path, error))? != bytes {
            return Err(invalid(format!("构建缓存内容已改变：{}", path.display())));
        }
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| io(path, error))?;
    file.write_all(bytes).map_err(|error| io(path, error))?;
    file.sync_all().map_err(|error| io(path, error))
}

fn npm_cli(node: &Path) -> Result<PathBuf> {
    let mut directories = vec![
        node.parent()
            .ok_or_else(|| invalid("Node 可执行文件缺少父目录"))?
            .to_path_buf(),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&path));
    }
    for directory in directories {
        for suffix in [
            "node_modules/npm/bin/npm-cli.js",
            "bin/node_modules/npm/bin/npm-cli.js",
            "../lib/node_modules/npm/bin/npm-cli.js",
        ] {
            let candidate = directory.join(suffix);
            if candidate.is_file() {
                return candidate
                    .canonicalize()
                    .map_err(|error| io(candidate, error));
            }
        }
        let npm = directory.join("npm");
        if let Ok(resolved) = npm.canonicalize()
            && resolved.file_name() == Some(OsStr::new("npm-cli.js"))
        {
            return Ok(resolved);
        }
    }
    Err(invalid(
        "未找到 npm-cli.js；自定义客户端需要 Node.js 配套 npm",
    ))
}

async fn npm_command(
    node: &Path,
    npm: &Path,
    directory: &Path,
    arguments: &[&str],
    windows_null: bool,
) -> Result<()> {
    let mut args = vec![npm.as_os_str().to_owned()];
    args.extend(arguments.iter().map(|argument| {
        OsString::from(if windows_null && *argument == "/dev/null" {
            "NUL"
        } else {
            argument
        })
    }));
    run(node, &args, directory, "锁定 npm 依赖准备", 180)
        .await
        .map(|_| ())
}

async fn install_project_dependencies(project: &Project, node: &Path) -> Result<()> {
    let path = project.root.join("package.json");
    if !path.is_file() {
        return Ok(());
    }
    let original = fs::read(&path).map_err(|error| io(&path, error))?;
    let manifest: Value = serde_json::from_slice(&original)?;
    for key in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        if let Some(entries) = manifest.get(key).and_then(Value::as_object) {
            for (name, version) in entries {
                let version = version
                    .as_str()
                    .ok_or_else(|| invalid("npm 依赖版本必须是字符串"))?;
                if version.is_empty()
                    || version.contains([':', '/', '\\', '#'])
                    || version.chars().any(char::is_control)
                {
                    return Err(invalid(format!(
                        "依赖 {name} 仅允许 npm registry 版本；不能自动安装 Git、文件或工作区来源"
                    )));
                }
            }
        }
    }
    let npm = npm_cli(node)?;
    if !project.root.join("package-lock.json").is_file() {
        npm_command(
            node,
            &npm,
            &project.root,
            &[
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ],
            false,
        )
        .await?;
    }
    npm_command(
        node,
        &npm,
        &project.root,
        &[
            "ci",
            "--ignore-scripts",
            "--include=dev",
            "--no-audit",
            "--no-fund",
        ],
        false,
    )
    .await?;
    if fs::read(&path).map_err(|error| io(&path, error))? != original {
        return Err(invalid("npm 改变了用户 package.json；请检查改动后重新构建"));
    }
    Ok(())
}

async fn drain<R: tokio::io::AsyncRead + Unpin>(mut input: R) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = input.read(&mut buffer).await?;
        if count == 0 {
            return Ok(result);
        }
        let keep = count.min((256 * 1024_usize).saturating_sub(result.len()));
        result.extend_from_slice(&buffer[..keep]);
    }
}
async fn run(
    executable: impl AsRef<OsStr>,
    args: &[OsString],
    directory: &Path,
    operation: &str,
    seconds: u64,
) -> Result<String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(directory)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env("NODE_ENV", "production")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("子进程缺少 stdout 管道"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("子进程缺少 stderr 管道"))?;
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
        tokio::join!(child.wait(), drain(stdout), drain(stderr))
    })
    .await;
    let (status, out, err) = match result {
        Ok(value) => value,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(ClientBuildError::Timeout {
                operation: operation.into(),
                seconds,
            });
        }
    };
    let status = status.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?;
    let out = String::from_utf8_lossy(&out.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?)
    .into_owned();
    let err = String::from_utf8_lossy(&err.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?)
    .into_owned();
    if !status.success() {
        return Err(ClientBuildError::Process {
            operation: operation.into(),
            code: status.code(),
            output: format!("{out}\n{err}"),
        });
    }
    if !err.trim().is_empty() {
        tracing::warn!(target:"kenvoy::build", "{}",err.trim());
    }
    Ok(out)
}

fn output(
    directory: PathBuf,
    mut dependencies: BTreeMap<String, String>,
) -> Result<ClientBuildOutput> {
    let package: Value = serde_json::from_slice(PACKAGE)?;
    for section in ["dependencies", "devDependencies"] {
        let entries = package[section]
            .as_object()
            .ok_or_else(|| invalid("内嵌框架依赖清单无效"))?;
        for (name, value) in entries {
            let version = value
                .as_str()
                .ok_or_else(|| invalid("内嵌框架依赖版本无效"))?;
            let key = format!("framework:{name}");
            if dependencies
                .get(&key)
                .is_some_and(|actual| actual != version)
            {
                return Err(invalid(format!("框架依赖 {name} 的实际版本不匹配锁定清单")));
            }
            dependencies.insert(key, version.into());
        }
    }
    dependencies.insert("framework:npm_lock_sha256".into(), digest(LOCK));
    fn walk(root: &Path, path: &Path, files: &mut BTreeMap<String, ClientAsset>) -> Result<()> {
        for item in fs::read_dir(path).map_err(|error| io(path, error))? {
            let item = item.map_err(|error| io(path, error))?;
            let path = item.path();
            let kind = item.file_type().map_err(|error| io(&path, error))?;
            if kind.is_symlink() {
                return Err(invalid("客户端产物不能是链接"));
            }
            if kind.is_dir() {
                walk(root, &path, files)?;
            } else if kind.is_file() {
                let bytes = fs::read(&path).map_err(|error| io(&path, error))?;
                let name = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("客户端产物越界"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(
                    name,
                    ClientAsset {
                        size: bytes.len() as u64,
                        sha256: digest(&bytes),
                    },
                );
            } else {
                return Err(invalid("客户端产物不是普通文件"));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(&directory, &directory, &mut files)?;
    if !files.contains_key("index.html") {
        return Err(invalid("客户端构建缺少 index.html"));
    }
    let fingerprint = digest(&serde_json::to_vec(&files)?);
    Ok(ClientBuildOutput {
        client_dir: directory,
        fingerprint,
        files,
        dependencies,
        seed: None,
    })
}
