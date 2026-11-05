//! 发行构建把通用 React 资产编入唯一的用户 Binary，不在应用启动时下载依赖。
use sha2::{Digest, Sha256};
use std::{
    env,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

enum BuildLayout {
    Workspace { root: PathBuf, kengine: PathBuf },
    Package { root: PathBuf },
}

impl BuildLayout {
    fn detect(package: &Path, manifest: &toml::Value) -> io::Result<Self> {
        let dependency = manifest
            .get("dependencies")
            .and_then(|dependencies| dependencies.get("kengine"))
            .ok_or_else(|| io::Error::other("Kenvoy manifest 缺少 kengine 依赖"))?;
        let Some(path) = dependency.get("path") else {
            let version = dependency
                .as_str()
                .or_else(|| dependency.get("version").and_then(toml::Value::as_str));
            if !version.is_some_and(|version| !version.trim().is_empty()) {
                return Err(io::Error::other("独立包的 kengine 依赖缺少版本约束"));
            }
            return Ok(Self::Package {
                root: package.to_path_buf(),
            });
        };
        let path = path
            .as_str()
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| io::Error::other("kengine 本地依赖路径无效"))?;
        let root = package
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| io::Error::other("本地框架 Workspace 不存在"))?;
        let kengine = package.join(path).canonicalize()?;
        let workspace = read_toml(&root.join("Cargo.toml"))?;
        let members = workspace
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
            .ok_or_else(|| io::Error::other("本地框架 manifest 缺少 Workspace 成员"))?;
        for member in [package, kengine.as_path()] {
            let relative = member
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_str()
                .ok_or_else(|| io::Error::other("Workspace 成员路径不是 Unicode"))?
                .replace('\\', "/");
            if !members
                .iter()
                .any(|member| member.as_str() == Some(relative.as_str()))
            {
                return Err(io::Error::other(format!(
                    "本地框架不是声明的 Workspace 成员：{relative}"
                )));
            }
        }
        Ok(Self::Workspace {
            root: root.to_path_buf(),
            kengine,
        })
    }

    fn root(&self) -> &Path {
        match self {
            Self::Workspace { root, .. } | Self::Package { root } => root,
        }
    }
}

struct BuildMetadata {
    layout: BuildLayout,
    cargo_lock_sha256: String,
    kengine_version: String,
}

impl BuildMetadata {
    fn read(package: &Path) -> io::Result<Self> {
        let manifest = read_toml(&package.join("Cargo.toml"))?;
        let layout = BuildLayout::detect(package, &manifest)?;
        let lock_path = layout.root().join("Cargo.lock");
        let lock_source = fs::read_to_string(&lock_path)?;
        let lock = parse_toml(&lock_path, &lock_source)?;
        let kenvoy_version = env::var("CARGO_PKG_VERSION").map_err(io::Error::other)?;
        let kengine = locked_kengine(&lock, &kenvoy_version)?;
        let kengine_version = kengine
            .get("version")
            .and_then(toml::Value::as_str)
            .filter(|version| !version.trim().is_empty())
            .ok_or_else(|| io::Error::other("锁文件中 kengine 的版本无效"))?;
        match &layout {
            BuildLayout::Workspace { kengine: path, .. } => {
                let manifest = read_toml(&path.join("Cargo.toml"))?;
                let package = manifest
                    .get("package")
                    .ok_or_else(|| io::Error::other("kengine manifest 缺少 package"))?;
                if package.get("name").and_then(toml::Value::as_str) != Some("kengine")
                    || package.get("version").and_then(toml::Value::as_str) != Some(kengine_version)
                    || kengine.get("source").is_some()
                {
                    return Err(io::Error::other(
                        "Workspace 的 kengine manifest 与锁定本地依赖不一致",
                    ));
                }
            }
            BuildLayout::Package { .. } => {
                if !kengine
                    .get("source")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|source| source.starts_with("registry+"))
                {
                    return Err(io::Error::other(
                        "独立包锁文件必须包含真实的 kengine registry 依赖",
                    ));
                }
            }
        }
        let cargo_lock_sha256 = Sha256::digest(lock_source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            layout,
            cargo_lock_sha256,
            kengine_version: kengine_version.to_owned(),
        })
    }
}

fn parse_toml(path: &Path, source: &str) -> io::Result<toml::Value> {
    toml::from_str(source)
        .map_err(|error| io::Error::other(format!("无法解析 {}：{error}", path.display())))
}

fn read_toml(path: &Path) -> io::Result<toml::Value> {
    parse_toml(path, &fs::read_to_string(path)?)
}

fn locked_kengine<'a>(lock: &'a toml::Value, kenvoy_version: &str) -> io::Result<&'a toml::Value> {
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| io::Error::other("框架锁文件缺少 package 列表"))?;
    let owners: Vec<_> = packages
        .iter()
        .filter(|package| {
            package.get("name").and_then(toml::Value::as_str) == Some("kenvoy")
                && package.get("version").and_then(toml::Value::as_str) == Some(kenvoy_version)
                && package.get("source").is_none()
        })
        .collect();
    let [owner] = owners.as_slice() else {
        return Err(io::Error::other(
            "框架锁文件没有唯一匹配当前版本的 Kenvoy 根包",
        ));
    };
    let dependencies = owner
        .get("dependencies")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| io::Error::other("框架锁文件的 Kenvoy 缺少依赖列表"))?;
    let mut references = Vec::new();
    for dependency in dependencies {
        let reference = dependency
            .as_str()
            .ok_or_else(|| io::Error::other("框架锁文件的依赖标识不是字符串"))?;
        if reference.split_whitespace().next() == Some("kengine") {
            references.push(reference);
        }
    }
    let [reference] = references.as_slice() else {
        return Err(io::Error::other(
            "框架锁文件的 Kenvoy 没有唯一 kengine 依赖",
        ));
    };
    let engines: Vec<_> = packages
        .iter()
        .filter(|package| {
            if package.get("name").and_then(toml::Value::as_str) != Some("kengine") {
                return false;
            }
            let Some(version) = package.get("version").and_then(toml::Value::as_str) else {
                return false;
            };
            if *reference == "kengine" || *reference == format!("kengine {version}") {
                return true;
            }
            package
                .get("source")
                .and_then(toml::Value::as_str)
                .is_some_and(|source| *reference == format!("kengine {version} ({source})"))
        })
        .collect();
    let [engine] = engines.as_slice() else {
        return Err(io::Error::other(
            "框架锁文件无法唯一解析 Kenvoy 的 kengine 版本",
        ));
    };
    Ok(*engine)
}

fn source_fingerprint(package: &Path, layout: &BuildLayout) -> io::Result<String> {
    fn walk(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(io::Error::other("框架源码不能包含链接"));
            }
            if kind.is_dir() {
                walk(&entry.path(), files)?;
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
        Ok(())
    }
    let mut files = vec![
        layout.root().join("Cargo.lock"),
        package.join("Cargo.toml"),
        package.join("build.rs"),
        package.join("frontend/scripts/client-builder.mjs"),
        package.join("frontend/scripts/client-dev.mjs"),
    ];
    let mut directories = vec![package.join("src"), package.join("frontend/src")];
    if let BuildLayout::Workspace { root, kengine } = layout {
        files.push(root.join("Cargo.toml"));
        files.push(kengine.join("Cargo.toml"));
        directories.push(kengine.join("src"));
    }
    for directory in directories {
        println!("cargo:rerun-if-changed={}", directory.display());
        walk(&directory, &mut files)?;
    }
    for entry in fs::read_dir(package.join("frontend"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .strip_prefix(layout.root())
            .map_err(io::Error::other)?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(path)?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect(
    root: &Path,
    path: &Path,
    files: &mut Vec<(String, PathBuf)>,
    include: fn(&str) -> bool,
) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other("前端构建资产不能包含符号链接"));
        }
        if kind.is_dir() {
            collect(root, &entry.path(), files, include)?;
        } else if kind.is_file() {
            let full = entry.path();
            let relative = full
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_str()
                .ok_or_else(|| io::Error::other("资产路径不是 Unicode"))?
                .replace('\\', "/");
            if include(&relative) {
                files.push((relative, full));
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("缺少 Cargo 包目录")?)
        .canonicalize()?;
    let metadata = BuildMetadata::read(&package)?;
    let framework_fingerprint = source_fingerprint(&package, &metadata.layout)?;
    let root = package.join("frontend/dist");
    println!("cargo:rerun-if-changed=frontend/dist");
    if !root.join("index.html").is_file() {
        return Err("缺少通用 React 发行资产。请先在 crates/kenvoy/frontend 执行 npm ci --ignore-scripts 与 npm run build，再编译 Kenvoy".into());
    }
    let mut files = Vec::new();
    collect(&root, &root, &mut files, |path| {
        path == "index.html" || path.starts_with("assets/")
    })?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if !files.iter().any(|(path, _)| path.ends_with(".js")) {
        return Err("前端构建没有生成 JavaScript 资产".into());
    }
    let types_root = root.join("types");
    if !types_root.is_dir() {
        return Err(
            "缺少编辑器 SDK 类型。请在 crates/kenvoy/frontend 执行 npm run build 后重新编译".into(),
        );
    }
    let mut types = Vec::new();
    collect(&types_root, &types_root, &mut types, |path| {
        path.ends_with(".d.ts")
    })?;
    types.sort_by(|a, b| a.0.cmp(&b.0));
    for required in [
        "sdk.d.ts",
        "presentation/jsx-runtime.d.ts",
        "presentation/jsx-dev-runtime.d.ts",
    ] {
        if !types.iter().any(|(name, _)| name == required) {
            return Err(format!("编辑器类型缺少 {required}；请重新构建前端").into());
        }
    }
    let mut output = String::new();
    for (constant, assets) in [("BUNDLED_ASSETS", files), ("EDITOR_TYPES", types)] {
        writeln!(output, "pub const {constant}: &[(&str, &[u8])] = &[")?;
        for (name, path) in assets {
            writeln!(
                output,
                "({name:?}, include_bytes!({:?})),",
                path.to_str().ok_or("资产绝对路径不是 Unicode")?
            )?;
        }
        output.push_str("];\n");
    }
    output.push_str(&format!(
        "pub const FRAMEWORK_FINGERPRINT: &str = {framework_fingerprint:?};\n"
    ));
    output.push_str(&format!(
        "pub(crate) const CARGO_LOCK_SHA256: &str = {:?};\n\
         pub(crate) const KENGINE_VERSION: &str = {:?};\n",
        metadata.cargo_lock_sha256, metadata.kengine_version
    ));
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 Cargo 输出目录")?)
        .join("frontend_assets.rs");
    fs::write(out, output)?;
    Ok(())
}
