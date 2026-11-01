//! 发行构建把通用 React 资产编入唯一的用户 Binary，不在应用启动时下载依赖。
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

fn source_fingerprint(package: &Path) -> io::Result<String> {
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
    let workspace = package
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::other("框架 Workspace 不存在"))?;
    let mut files = vec![
        workspace.join("Cargo.lock"),
        workspace.join("Cargo.toml"),
        package.join("Cargo.toml"),
        package.join("build.rs"),
        package.join("frontend/scripts/client-builder.mjs"),
        package.join("frontend/scripts/client-dev.mjs"),
        workspace.join("crates/kengine/Cargo.toml"),
    ];
    for directory in [
        package.join("src"),
        workspace.join("crates/kengine/src"),
        package.join("frontend/src"),
    ] {
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
            .strip_prefix(workspace)
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

fn collect(root: &Path, path: &Path, files: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other("前端构建资产不能包含符号链接"));
        }
        if kind.is_dir() {
            collect(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let full = entry.path();
            let relative = full
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_str()
                .ok_or_else(|| io::Error::other("资产路径不是 Unicode"))?
                .replace('\\', "/");
            if relative == "index.html" || relative.starts_with("assets/") {
                files.push((relative, full));
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("缺少 Cargo 包目录")?);
    let framework_fingerprint = source_fingerprint(&package)?;
    let root = package.join("frontend/dist");
    println!("cargo:rerun-if-changed=frontend/dist");
    if !root.join("index.html").is_file() {
        return Err("缺少通用 React 发行资产。请先在 crates/kenvoy/frontend 执行 npm ci --ignore-scripts 与 npm run build，再编译 Kenvoy".into());
    }
    let mut files = Vec::new();
    collect(&root, &root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if !files.iter().any(|(path, _)| path.ends_with(".js")) {
        return Err("前端构建没有生成 JavaScript 资产".into());
    }
    let mut output = String::from("pub const BUNDLED_ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        output.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            path.to_str().ok_or("资产绝对路径不是 Unicode")?
        ));
    }
    output.push_str("];\n");
    output.push_str(&format!(
        "pub const FRAMEWORK_FINGERPRINT: &str = {framework_fingerprint:?};\n"
    ));
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少 Cargo 输出目录")?)
        .join("frontend_assets.rs");
    fs::write(out, output)?;
    Ok(())
}
