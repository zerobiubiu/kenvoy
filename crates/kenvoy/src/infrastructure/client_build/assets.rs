use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    ClientAsset, ClientBuildOutput, Result, digest,
    error::{invalid, io},
    sources::{LOCK, PACKAGE},
};
use serde_json::Value;

pub(super) fn output(
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
