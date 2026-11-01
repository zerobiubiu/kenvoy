use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    #[error("源码分析失败，共 {} 项诊断", .0.len())]
    Language(Vec<kengine::Diagnostic>),
    #[error("应用声明分析失败，共 {} 项诊断", .0.len())]
    Dsl(Vec<crate::domain::source::DslDiagnostic>),
    #[error("序列化失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Record(#[from] crate::domain::validation::ValidationError),
    #[error(transparent)]
    Fields(#[from] crate::domain::validation::ValidationErrors),
    #[error(transparent)]
    Database(#[from] crate::infrastructure::persistence::sqlite::DatabaseError),
    #[error(transparent)]
    Migration(#[from] crate::infrastructure::persistence::sqlite::migration::MigrationError),
    #[error(transparent)]
    Auth(
        #[from]
        crate::application::identity::AuthError<
            crate::infrastructure::persistence::sqlite::DatabaseError,
        >,
    ),
    #[error(transparent)]
    Http(#[from] crate::interfaces::http::SetupError),
    #[error(transparent)]
    Frontend(#[from] crate::infrastructure::frontend::FrontendError),
    #[error(transparent)]
    Artifact(#[from] crate::infrastructure::artifacts::ArtifactError),
    #[error(transparent)]
    ClientBuild(#[from] crate::infrastructure::client_build::ClientBuildError),
    #[error(transparent)]
    JavascriptDiagnostic(#[from] crate::application::execution::JsDiagnostic),
    #[error(transparent)]
    Javascript(#[from] crate::application::execution::JsError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 启动与开发更新共享诊断输出；JSON 日志保留机器可读的位置和建议。
pub fn report_diagnostics(error: &Error) {
    match error {
        Error::Language(items) => {
            for item in items {
                let position = item.span.as_ref().map(|span| &span.start_position);
                tracing::error!(target:"kenvoy::compiler",code=%item.code,category=?item.category,
                    path=%item.path.display(),line=position.map(|value|value.line),column=position.map(|value|value.column),
                    suggestion=%item.suggestion,"{}；建议：{}",item.message,item.suggestion);
            }
        }
        Error::Dsl(items) => {
            for item in items {
                tracing::error!(target:"kenvoy::compiler",code=%item.code,category=?item.category,
                    path=%item.source.path.display(),line=item.source.span.start_position.line,column=item.source.span.start_position.column,
                    suggestion=%item.suggestion,"{}；建议：{}",item.message,item.suggestion);
            }
        }
        Error::JavascriptDiagnostic(item) => {
            let source = item.source.as_ref();
            tracing::error!(target:"kenvoy::compiler",code=%item.code,category="JavaScript",
                path=?source.map(|value| &value.path),line=source.map(|value|value.span.start_position.line),
                column=source.map(|value|value.span.start_position.column),suggestion=%item.suggestion,"{}；建议：{}",item.message,item.suggestion);
        }
        _ => {}
    }
}

pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}

impl From<crate::infrastructure::project::ProjectError> for Error {
    fn from(error: crate::infrastructure::project::ProjectError) -> Self {
        use crate::infrastructure::project::ProjectError;
        match error {
            ProjectError::Io { path, source } => Self::Io { path, source },
            ProjectError::Config { path, source } => Self::Config { path, source },
            ProjectError::InvalidConfig(message) => Self::InvalidConfig(message),
            ProjectError::Engine(error) => Self::Engine(error),
            ProjectError::Dsl(items) => Self::Dsl(items),
            ProjectError::Json(error) => Self::Json(error),
        }
    }
}

impl From<crate::application::compilation::CompilationError> for Error {
    fn from(error: crate::application::compilation::CompilationError) -> Self {
        use crate::application::compilation::CompilationError;
        match error {
            CompilationError::Language(items) => Self::Language(items),
            CompilationError::Dsl(items) => Self::Dsl(items),
        }
    }
}

impl From<crate::interfaces::cli::CliError> for Error {
    fn from(error: crate::interfaces::cli::CliError) -> Self {
        use crate::interfaces::cli::CliError;
        match error {
            CliError::Io { path, source } => Self::Io { path, source },
            CliError::InvalidConfig(message) => Self::InvalidConfig(message),
        }
    }
}
