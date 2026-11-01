use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("访问文件 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("配置 {path} 无效：{source}")]
    Config {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("配置约束错误：{0}")]
    InvalidConfig(String),
    #[error(transparent)]
    Engine(#[from] kengine::EngineError),
    #[error("应用声明分析失败，共 {} 项诊断", .0.len())]
    Dsl(Vec<crate::domain::source::DslDiagnostic>),
    #[error("序列化失败：{0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ProjectError>;

pub(super) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.into(),
        source,
    }
}
