use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error("访问文件 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("配置约束错误：{0}")]
    InvalidConfig(String),
}

pub(super) type Result<T> = std::result::Result<T, CliError>;

pub(super) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> CliError {
    CliError::Io {
        path: path.into(),
        source,
    }
}
