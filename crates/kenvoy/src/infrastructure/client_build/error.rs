use std::path::PathBuf;

use crate::application::execution::JsDiagnostic;

#[derive(Debug, thiserror::Error)]
pub enum ClientBuildError {
    #[error("客户端构建文件 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("客户端构建边界无效：{0}")]
    Invalid(String),
    #[error("客户端构建数据无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Slice(#[from] JsDiagnostic),
    #[error(transparent)]
    Frontend(#[from] crate::infrastructure::frontend::FrontendError),
    #[error(transparent)]
    Artifact(#[from] crate::infrastructure::artifacts::ArtifactError),
    #[error("{operation} 进程启动失败：{source}")]
    Spawn {
        operation: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation} 超过 {seconds} 秒限制，进程已停止")]
    Timeout { operation: String, seconds: u64 },
    #[error("{operation} 失败（退出码 {code:?}）：{output}")]
    Process {
        operation: String,
        code: Option<i32>,
        output: String,
    },
}
pub type Result<T> = std::result::Result<T, ClientBuildError>;
pub(super) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> ClientBuildError {
    ClientBuildError::Io {
        path: path.into(),
        source,
    }
}
pub(super) fn invalid(message: impl Into<String>) -> ClientBuildError {
    ClientBuildError::Invalid(message.into())
}
