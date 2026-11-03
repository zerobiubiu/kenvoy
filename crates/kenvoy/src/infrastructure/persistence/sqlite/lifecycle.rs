//! 文件锁覆盖迁移、工作队列和整个服务周期；不删除锁文件，不依靠 PID 判断存活。
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use super::{DatabaseError, Result};

pub struct DatabaseLease {
    _file: File,
}

pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn io(path: &Path, source: std::io::Error) -> DatabaseError {
    DatabaseError::Io {
        path: path.into(),
        source,
    }
}

pub fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(DatabaseError::InvalidState(format!(
                "数据库附属文件必须是普通文件：{}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(path, error)),
    }
}

impl DatabaseLease {
    pub fn acquire(path: &Path) -> Result<Self> {
        if path.as_os_str() == OsStr::new(":memory:") {
            return Err(DatabaseError::InvalidState(
                "生命周期锁要求独立持久化路径".into(),
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
        }
        let lock_path = sidecar(path, ".kenvoy.lock");
        reject_link(&lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io(&lock_path, error))?;
        file.try_lock().map_err(|error| DatabaseError::InvalidState(format!(
            "数据库正在由另一进程使用，拒绝并行迁移或新旧服务混用：{}（{error}）；请先停止旧服务。此锁不能协调尚未实现本协议的旧 CLI", path.display()
        )))?;
        Ok(Self { _file: file })
    }
}
