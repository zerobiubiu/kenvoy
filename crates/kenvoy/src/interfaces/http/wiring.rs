//! 具体驱动绑定仅位于此装配接缝，控制器通过应用用例工作。

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    extract::Request,
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use tokio::sync::{RwLock, watch};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use url::Url;

use crate::{
    application::{
        execution::JsError,
        identity::{Auth, AuthError},
        publication::{Publication, PublicationError},
        resources::{ResourceRepository, ResourceService},
    },
    domain::model::ApplicationIR,
    infrastructure::{
        identity::SystemCredentials,
        javascript::JavascriptRuntime,
        persistence::sqlite::{Database, DatabaseError},
    },
};

use super::{ApiError, HttpRuntime, Shared};

pub(super) type Repository = Database;
pub(super) type RepositoryError = DatabaseError;
pub(super) type HttpAuth = Auth<Database, SystemCredentials>;
pub(super) type HttpAuthError = AuthError<DatabaseError>;
pub(super) type Published = Publication<Database, JavascriptRuntime>;
pub(super) type Resources = ResourceService<Database>;

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("HTTP 配置无效：{0}")]
    Configuration(String),
    #[error("前端资源目录 {path} 无法访问：{source}")]
    Assets {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("应用发布失败：{0}")]
    Publication(String),
    #[error(transparent)]
    Javascript(#[from] JsError),
}

impl From<PublicationError<DatabaseError>> for SetupError {
    fn from(error: PublicationError<DatabaseError>) -> Self {
        match error {
            PublicationError::Repository(error) => Self::Database(error),
            PublicationError::Publication(message) => Self::Publication(message),
        }
    }
}

impl HttpRuntime {
    pub async fn new(
        application: ApplicationIR,
        database: Database,
        auth: HttpAuth,
        origin: String,
        assets: Option<PathBuf>,
    ) -> Result<Self, SetupError> {
        let parsed = Url::parse(&origin).map_err(|_| {
            SetupError::Configuration("public_origin 必须为完整 HTTP(S) origin".into())
        })?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.origin().ascii_serialization() != origin
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(SetupError::Configuration(
                "public_origin 不能包含凭据、路径、查询、片段或尾部斜杠".into(),
            ));
        }
        if parsed.scheme() == "http"
            && !parsed.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            })
        {
            return Err(SetupError::Configuration(
                "非回环 public_origin 必须使用 HTTPS".into(),
            ));
        }
        let authority = origin
            .split_once("://")
            .map(|(_, value)| value.to_owned())
            .ok_or_else(|| SetupError::Configuration("public_origin 缺少主机".into()))?;
        let generation = ResourceRepository::generation(&database).await?;
        let assets = canonical_assets(assets)?;
        Ok(Self {
            shared: Arc::new(Shared {
                model: Publication::new(application, database.clone(), generation, assets),
                resources: ResourceService::new(database),
                auth,
                origin,
                authority,
                secure_cookie: parsed.scheme() == "https",
                request_limits: Mutex::new(BTreeMap::new()),
                development: RwLock::new(None),
                development_changed: watch::channel(0).0,
            }),
        })
    }

    /// 服务启动前附加与当前应用注册表一致的函数包。
    pub async fn with_javascript(self, javascript: JavascriptRuntime) -> Result<Self, SetupError> {
        self.shared.model.with_executor(javascript).await?;
        Ok(self)
    }

    /// 同代读写屏障与已接受发布任务由应用用例统一持有。
    pub async fn publish_prepared(
        &self,
        application: ApplicationIR,
        assets: Option<PathBuf>,
        javascript: Option<JavascriptRuntime>,
    ) -> Result<u64, SetupError> {
        let assets = canonical_assets(assets)?;
        Ok(self
            .shared
            .model
            .publish_prepared(application, assets, javascript)
            .await?)
    }
}

fn canonical_assets(path: Option<PathBuf>) -> Result<Option<PathBuf>, SetupError> {
    path.map(|path| {
        let canonical = path.canonicalize().map_err(|source| SetupError::Assets {
            path: path.clone(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(SetupError::Configuration("前端资源位置必须为目录".into()));
        }
        Ok(canonical)
    })
    .transpose()
}

pub(super) async fn serve_file(
    root: &std::path::Path,
    asset: &str,
    method: Method,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let file = root
        .join(asset)
        .canonicalize()
        .map_err(|_| ApiError::missing())?;
    if !file.starts_with(root) || !file.is_file() {
        return Err(ApiError::missing());
    }
    let mut request = Request::new(axum::body::Body::empty());
    *request.method_mut() = method;
    *request.headers_mut() = headers;
    let response = ServeFile::new(file).oneshot(request).await.map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "KV5050",
            "静态资源读取失败",
        )
    })?;
    Ok(response.map(axum::body::Body::new))
}
