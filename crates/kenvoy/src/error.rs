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
    Dsl(Vec<crate::dsl::DslDiagnostic>),
    #[error("序列化失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Record(#[from] crate::validation::ValidationError),
    #[error(transparent)]
    Fields(#[from] crate::validation::ValidationErrors),
    #[error(transparent)]
    Database(#[from] crate::database::DatabaseError),
    #[error(transparent)]
    Migration(#[from] crate::database::migration::MigrationError),
    #[error(transparent)]
    Auth(#[from] crate::auth::AuthError),
    #[error(transparent)]
    Http(#[from] crate::http::SetupError),
    #[error(transparent)]
    Frontend(#[from] crate::frontend::FrontendError),
    #[error(transparent)]
    Artifact(#[from] crate::artifact::ArtifactError),
    #[error(transparent)]
    ClientBuild(#[from] crate::client_build::ClientBuildError),
    #[error(transparent)]
    JavascriptDiagnostic(#[from] crate::javascript::JsDiagnostic),
    #[error(transparent)]
    Javascript(#[from] crate::javascript::JsError),
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
