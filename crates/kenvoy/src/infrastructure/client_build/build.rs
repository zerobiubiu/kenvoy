use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use kengine::ProgramIR;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    ClientBuildOptions, ClientBuildOutput, DevSeed, Result,
    assets::output,
    cache::{directory, immutable, reject_link},
    command::{install_project_dependencies, npm_cli, npm_command, run},
    digest,
    error::{invalid, io},
    graph::{private_modules, verify_sources},
    sources::{LOCK, PACKAGE, SOURCES},
};
use crate::{
    application::compilation::client_slice::{ClientImportTarget, client_slice},
    domain::model::{ApplicationIR, ModuleReferenceKind},
    infrastructure::{javascript::SwcClientCodeTransformer, project::Project},
};

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
        crate::infrastructure::frontend::export(&destination)?;
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
    let mut identity = Sha256::new();
    for bytes in [PACKAGE, LOCK] {
        identity.update((bytes.len() as u64).to_le_bytes());
        identity.update(bytes);
    }
    let hash = identity.finalize();
    let mut identity = String::with_capacity(hash.len() * 2);
    for byte in hash {
        write!(identity, "{byte:02x}").expect("写入字符串不会失败");
    }
    let toolchain = directory(
        &root,
        &cache.join(format!(
            "{}-{}-{}-node{}",
            &identity[..24],
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
    let marker = json!({"manifest_lock":identity,"lock":digest(LOCK),"node_major":node.version.split('.').next(),"platform":node.platform,"architecture":node.architecture});
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
            let slice = client_slice(program, app, reference, &SwcClientCodeTransformer)?;
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
