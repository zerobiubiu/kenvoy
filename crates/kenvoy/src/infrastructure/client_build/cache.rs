use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use super::{
    Result,
    error::{invalid, io},
};

pub(super) fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid("构建缓存不能通过符号链接替换"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}
pub(super) fn directory(root: &Path, path: &Path) -> Result<PathBuf> {
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
pub(super) fn immutable(path: &Path, bytes: &[u8]) -> Result<()> {
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
