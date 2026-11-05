//! 编辑器缓存只允许项目内非链接目录；临时文件只清理本次独占创建的内容。
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::super::error::{ProjectError as Error, Result, io};

const MAX_FILE: u64 = 32 * 1024 * 1024;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = metadata.file_type().is_symlink();
            if linked {
                return Err(Error::InvalidConfig(format!(
                    "编辑器文件及目录不能是链接或重解析点：{}",
                    path.display()
                )));
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(path, error)),
    }
}

pub(super) fn directory(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(Error::InvalidConfig(
                "编辑器目录必须是项目内普通相对路径".into(),
            ));
        };
        current.push(part);
        match metadata(&current)? {
            Some(metadata) if !metadata.is_dir() => {
                return Err(Error::InvalidConfig(format!(
                    "编辑器缓存位置不是目录：{}",
                    current.display()
                )));
            }
            Some(_) => {}
            None => fs::create_dir(&current).map_err(|error| io(&current, error))?,
        }
        // 每一层重新解析，拒绝 Windows junction 等目录越界。
        current = current
            .canonicalize()
            .map_err(|error| io(&current, error))?;
        if !current.starts_with(root) {
            return Err(Error::InvalidConfig("编辑器缓存目录越过项目根".into()));
        }
    }
    Ok(current)
}

pub(super) fn path(root: &Path, name: &str) -> Result<PathBuf> {
    let relative = Path::new(name);
    if name.contains('\\')
        || relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::InvalidConfig(format!("编辑器文件路径无效：{name}")));
    }
    let checked = super::super::paths::safe_path(root, relative)?;
    let mut current = root.to_path_buf();
    let mut parts = relative.components().peekable();
    while let Some(part) = parts.next() {
        current.push(part.as_os_str());
        if let Some(metadata) = metadata(&current)? {
            let valid = if parts.peek().is_some() {
                metadata.is_dir()
            } else {
                metadata.is_file()
            };
            if !valid {
                return Err(Error::InvalidConfig(format!(
                    "编辑器路径类型无效，要求普通目录及普通文件：{}",
                    current.display()
                )));
            }
        }
    }
    Ok(checked)
}

pub(super) fn read(root: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let path = path(root, name)?;
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(&path, error)),
    };
    let metadata = file.metadata().map_err(|error| io(&path, error))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE {
        return Err(Error::InvalidConfig(format!(
            "编辑器生成文件不是普通文件或超过 32 MiB：{}",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io(&path, error))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(Error::InvalidConfig(
            "读取过程中编辑器生成文件超过 32 MiB".into(),
        ));
    }
    Ok(Some(bytes))
}

struct Temporary {
    path: PathBuf,
    pending: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if self.pending {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn replace(
    root: &Path,
    name: &str,
    expected: Option<&[u8]>,
    bytes: &[u8],
) -> Result<()> {
    let destination = path(root, name)?;
    if let Some(parent) = Path::new(name)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        directory(root, parent)?;
    }
    let number = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    let temporary = destination.with_file_name(format!(
        ".kenvoy-editor-{}-{number}.tmp",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io(&temporary, error))?;
    let mut temporary = Temporary {
        path: temporary,
        pending: true,
    };
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io(&temporary.path, error))?;
    drop(file);
    if read(root, name)?.as_deref() != expected {
        return Err(Error::InvalidConfig(format!(
            "编辑器文件在生成期间发生变化，未覆盖：{name}"
        )));
    }
    let destination = path(root, name)?;
    fs::rename(&temporary.path, &destination).map_err(|error| io(&destination, error))?;
    temporary.pending = false;
    Ok(())
}

pub(super) fn remove(root: &Path, name: &str, expected: &[u8]) -> Result<()> {
    if read(root, name)?.as_deref() != Some(expected) {
        return Err(Error::InvalidConfig(format!(
            "旧编辑器声明发生变化，未删除：{name}"
        )));
    }
    let path = path(root, name)?;
    fs::remove_file(&path).map_err(|error| io(&path, error))
}

pub(super) fn create_configuration(root: &Path, name: &str, bytes: &[u8]) -> Result<bool> {
    let path = path(root, name)?;
    if metadata(&path)?.is_some() {
        return Ok(false);
    }
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            self::path(root, name)?;
            return Ok(false);
        }
        Err(error) => return Err(io(&path, error)),
    };
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io(&path, error))?;
    Ok(true)
}
