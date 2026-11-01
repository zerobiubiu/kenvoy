//! 框架内置 CSR 资产导出到独占构建目录，业务数据不进入此目录。
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

include!(concat!(env!("OUT_DIR"), "/frontend_assets.rs"));

#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    #[error("前端资产 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("前端资产边界无效：{0}")]
    Boundary(String),
    #[error("内置前端资产内容已改变：{0}；请保留现场并重新构建")]
    Corrupted(PathBuf),
}
type Result<T> = std::result::Result<T, FrontendError>;
fn io(path: impl Into<PathBuf>, source: std::io::Error) -> FrontendError {
    FrontendError::Io {
        path: path.into(),
        source,
    }
}

pub fn fingerprint() -> String {
    let mut hash = Sha256::new();
    for (name, bytes) in BUNDLED_ASSETS {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 将内置 Runtime 写入调用方独占创建的空 staging 目录；不覆盖任何文件。
pub fn export(destination: &Path) -> Result<()> {
    let directory = destination
        .canonicalize()
        .map_err(|error| io(destination, error))?;
    if fs::read_dir(&directory)
        .map_err(|error| io(&directory, error))?
        .next()
        .is_some()
    {
        return Err(FrontendError::Boundary("前端输出目录必须为空".into()));
    }
    for (name, bytes) in BUNDLED_ASSETS {
        let path = directory.join(name);
        if let Some(parent) = path.parent() {
            owned_directory(&directory, parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| io(&path, error))?;
        file.write_all(bytes).map_err(|error| io(&path, error))?;
        file.sync_all().map_err(|error| io(&path, error))?;
    }
    verify(&directory)
}

fn owned_directory(root: &Path, path: &Path) -> Result<PathBuf> {
    if !path.starts_with(root) {
        return Err(FrontendError::Boundary("缓存目录必须在项目根内".into()));
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| FrontendError::Boundary("无法解析缓存目录".into()))?;
    let mut current = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(FrontendError::Boundary("缓存路径包含非法组件".into()));
        };
        current.push(part);
        if !current.exists() {
            fs::create_dir(&current)
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        Ok(())
                    } else {
                        Err(error)
                    }
                })
                .map_err(|error| io(&current, error))?;
        }
        current = current
            .canonicalize()
            .map_err(|error| io(&current, error))?;
        if !current.starts_with(root) || !current.is_dir() {
            return Err(FrontendError::Boundary("缓存路径链接越界或不是目录".into()));
        }
    }
    Ok(current)
}

fn verify(directory: &Path) -> Result<()> {
    // Windows canonicalize 返回带扩展前缀的路径，比较双方必须采用同一形式。
    let directory = directory
        .canonicalize()
        .map_err(|error| io(directory, error))?;
    let directory = directory.as_path();
    fn inspect(directory: &Path, current: &Path) -> Result<()> {
        for entry in fs::read_dir(current).map_err(|error| io(current, error))? {
            let entry = entry.map_err(|error| io(current, error))?;
            let kind = entry.file_type().map_err(|error| io(entry.path(), error))?;
            if kind.is_symlink() {
                return Err(FrontendError::Boundary("内置缓存资产不能是链接".into()));
            }
            if kind.is_dir() {
                inspect(directory, &entry.path())?;
            } else {
                let path = entry.path();
                let relative = path
                    .strip_prefix(directory)
                    .map_err(|_| FrontendError::Boundary("缓存资产越界".into()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !kind.is_file() || !BUNDLED_ASSETS.iter().any(|(name, _)| *name == relative) {
                    return Err(FrontendError::Corrupted(path));
                }
            }
        }
        Ok(())
    }
    inspect(directory, directory)?;
    for (name, expected) in BUNDLED_ASSETS {
        let path = directory.join(name);
        let resolved = path.canonicalize().map_err(|error| io(&path, error))?;
        if !resolved.starts_with(directory) {
            return Err(FrontendError::Boundary("缓存资产链接越界".into()));
        }
        let actual = fs::read(&resolved).map_err(|error| io(&path, error))?;
        if actual != *expected {
            return Err(FrontendError::Corrupted(path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
