use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use super::{Asset, MAX_FILE, MAX_TOTAL, Result, digest, invalid, io};

pub(super) fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid(format!("产物路径不能是链接：{}", path.display())))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}
pub(super) fn check_directory(root: &Path, path: &Path) -> Result<()> {
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
pub(super) fn directory(root: &Path, path: &Path) -> Result<PathBuf> {
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
pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
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
pub(super) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
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
pub(super) fn collect(root: &Path) -> Result<BTreeMap<String, Asset>> {
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
                && name != "migrations.json"
                && name != "i18n.json"
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
