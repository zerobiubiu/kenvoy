use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::super::{
    error::{ProjectError as Error, Result, io},
    paths::safe_path,
};

const MAX_FILE: u64 = 32 * 1024 * 1024;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

pub(super) fn path(root: &Path, name: &str) -> Result<PathBuf> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(Error::InvalidConfig(format!(
                "国际化文件必须是非链接普通文件：{}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(&path, error)),
    }
    safe_path(root, Path::new(name))
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
            "国际化文件不是普通文件或超过 32 MiB 限制：{}",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io(&path, error))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(Error::InvalidConfig(format!(
            "读取过程中国际化文件超过 32 MiB 限制：{}",
            path.display()
        )));
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

pub(super) fn refresh_schema(root: &Path, bytes: &[u8]) -> Result<bool> {
    if read(root, "language.schema.json")?.as_deref() == Some(bytes) {
        return Ok(false);
    }
    let number = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    let temporary = root.join(format!(
        ".language.schema.{}.{number}.tmp",
        std::process::id()
    ));
    // 先独占创建，成功后才接管清理；绝不删除碰撞的既有文件。
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io(&temporary, error))?;
    let mut temporary = Temporary {
        path: temporary,
        pending: true,
    };
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io(&temporary.path, error));
    drop(file);
    written?;
    // 写入期间目标可能变化，提交前再次拒绝链接、目录与越界路径。
    let destination = path(root, "language.schema.json")?;
    fs::rename(&temporary.path, &destination).map_err(|error| io(&destination, error))?;
    temporary.pending = false;
    Ok(true)
}

pub(super) fn create_translations(root: &Path) -> Result<bool> {
    let path = path(root, "translations.json")?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(io(&path, error)),
    };
    file.write_all(b"{\n  \"$schema\": \"./language.schema.json\"\n}\n")
        .and_then(|()| file.sync_all())
        .map_err(|error| io(&path, error))?;
    Ok(true)
}
