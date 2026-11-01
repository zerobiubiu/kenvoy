//! HTTP 适配层只消费已验证模型；请求期间不解析源码、不初始化数据库。

mod development;
mod handlers;
mod projection;
mod uploads;
pub use development::{DevelopmentPhase, DevelopmentProxy};

use std::{
    collections::BTreeMap,
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use tokio::{
    net::TcpListener,
    sync::{RwLock, oneshot, watch},
    task::JoinHandle,
};
use url::Url;

use crate::{
    application::{ApplicationIR, ExecutionEnvironment, Role},
    auth::{Auth, AuthError, SessionView},
    database::{Database, DatabaseError},
    javascript::JavascriptRuntime,
    validation::ValidationError,
};

const BODY_LIMIT: usize = 1024 * 1024;
const COOKIE_NAME: &str = "kenvoy_session";
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    Javascript(#[from] crate::javascript::JsError),
}

struct Generation {
    application: ApplicationIR,
    generation: u64,
    assets: Option<PathBuf>,
    javascript: Option<JavascriptRuntime>,
}

struct Shared {
    model: RwLock<Generation>,
    database: Database,
    auth: Auth,
    origin: String,
    authority: String,
    secure_cookie: bool,
    request_limits: Mutex<BTreeMap<String, (Instant, u32)>>,
    publications: Mutex<Vec<JoinHandle<()>>>,
    closing: AtomicBool,
    development: RwLock<Option<development::Development>>,
    development_changed: watch::Sender<u64>,
}

#[derive(Clone, Copy)]
struct ClientPeer(Option<SocketAddr>);

#[derive(Clone)]
pub struct HttpRuntime {
    shared: Arc<Shared>,
}

impl HttpRuntime {
    pub async fn new(
        application: ApplicationIR,
        database: Database,
        auth: Auth,
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
        let generation = database.info().await?.generation;
        let assets = canonical_assets(assets)?;
        Ok(Self {
            shared: Arc::new(Shared {
                model: RwLock::new(Generation {
                    application,
                    generation,
                    assets,
                    javascript: None,
                }),
                database,
                auth,
                origin,
                authority,
                secure_cookie: parsed.scheme() == "https",
                request_limits: Mutex::new(BTreeMap::new()),
                publications: Mutex::new(vec![]),
                closing: AtomicBool::new(false),
                development: RwLock::new(None),
                development_changed: watch::channel(0).0,
            }),
        })
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/_kenvoy/session", get(handlers::session))
            .route("/_kenvoy/login", post(handlers::login))
            .route("/_kenvoy/logout", post(handlers::logout))
            .route("/_kenvoy/application", get(handlers::application))
            .route("/_kenvoy/development", get(development::status))
            .route("/_kenvoy/vite/hmr", get(development::websocket))
            .route("/_kenvoy/vite/{*path}", get(development::assets))
            .route(
                "/_kenvoy/uploads/{resource}/{field}",
                post(uploads::upload).layer(axum::extract::DefaultBodyLimit::disable()),
            )
            .route(
                "/_kenvoy/files/{id}",
                get(uploads::read).delete(uploads::remove),
            )
            .route("/api/actions/{name}", post(handlers::action))
            .route("/api/{resource}/options/{field}", post(handlers::options))
            .route(
                "/api/{resource}",
                get(handlers::list).post(handlers::create),
            )
            .route(
                "/api/{resource}/{id}",
                get(handlers::read)
                    .patch(handlers::update)
                    .delete(handlers::delete),
            )
            .fallback(handlers::fallback)
            .method_not_allowed_fallback(handlers::method_not_allowed)
            .layer(middleware::from_fn_with_state(self.shared.clone(), protect))
            .with_state(self.shared.clone())
    }

    /// 服务启动前附加与当前应用注册表一致的函数包。
    pub async fn with_javascript(self, javascript: JavascriptRuntime) -> Result<Self, SetupError> {
        {
            let mut model = self.shared.model.write().await;
            validate_javascript(&model.application, Some(&javascript))?;
            model.javascript = Some(javascript);
        }
        Ok(self)
    }

    /// 宿主负责在 HTTP 优雅退出完成后关闭数据库工作线程。
    pub async fn serve<F>(&self, listener: TcpListener, shutdown: F) -> std::io::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let shared = self.shared.clone();
        let result = axum::serve(
            listener,
            self.router()
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            shutdown.await;
            if let Err(error) = development::stop(&shared).await {
                tracing::error!(target:"kenvoy::http",%error,"开发连接关闭失败");
            }
        })
        .await;
        development::stop(&self.shared)
            .await
            .map_err(|_| std::io::Error::other("开发连接未能退出"))?;
        let publications = {
            let mut publications = self
                .shared
                .publications
                .lock()
                .map_err(|_| std::io::Error::other("应用发布队列锁损坏"))?;
            self.shared.closing.store(true, Ordering::Release);
            std::mem::take(&mut *publications)
        };
        for task in publications {
            task.await
                .map_err(|_| std::io::Error::other("应用发布任务异常退出"))?;
        }
        result
    }

    /// 所有请求持有同代读锁；迁移及模型/资源/函数发布在同一写锁中完成。
    pub async fn publish_prepared(
        &self,
        application: ApplicationIR,
        assets: Option<PathBuf>,
        javascript: Option<JavascriptRuntime>,
    ) -> Result<u64, SetupError> {
        let assets = canonical_assets(assets)?;
        validate_javascript(&application, javascript.as_ref())?;
        let (reply, response) = oneshot::channel();
        {
            let mut tasks = self
                .shared
                .publications
                .lock()
                .map_err(|_| SetupError::Publication("发布队列锁损坏".into()))?;
            tasks.retain(|task| !task.is_finished());
            if self.shared.closing.load(Ordering::Acquire) || tasks.len() >= 64 {
                return Err(SetupError::Publication("宿主正在关闭或发布队列已满".into()));
            }
            let shared = self.shared.clone();
            tasks.push(tokio::spawn(async move {
                let result = async {
                    let mut model = shared.model.write().await;
                    let generation = model
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| SetupError::Publication("应用代际编号已耗尽".into()))?;
                    shared
                        .database
                        .replace_application(application.clone(), None)
                        .await?;
                    *model = Generation {
                        application,
                        generation,
                        assets,
                        javascript,
                    };
                    Ok(generation)
                }
                .await;
                let _ = reply.send(result);
            }));
        }
        response
            .await
            .map_err(|_| SetupError::Publication("发布任务异常退出，状态需要核对".into()))?
    }
}

fn validate_javascript(
    application: &ApplicationIR,
    javascript: Option<&JavascriptRuntime>,
) -> Result<(), SetupError> {
    if application.actions.iter().any(|action| {
        matches!(
            action.runtime,
            ExecutionEnvironment::Server | ExecutionEnvironment::Shared
        ) && !javascript.is_some_and(|runtime| runtime.has_function(&action.handler))
    }) {
        return Err(SetupError::Publication(
            "服务端 Action 缺少对应代际的已编译函数".into(),
        ));
    }
    Ok(())
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

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    fields: Option<Vec<ValidationError>>,
}
impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            fields: None,
        }
    }
    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "KV5000", message)
    }
    fn missing() -> Self {
        Self::new(StatusCode::NOT_FOUND, "KV5004", "请求的资源不存在")
    }
    fn denied(role: Role) -> Self {
        if role == Role::Anonymous {
            Self::new(StatusCode::UNAUTHORIZED, "KV5001", "请先登录")
        } else {
            Self::new(StatusCode::FORBIDDEN, "KV5003", "当前用户无权执行此操作")
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let id = request_id();
        let mut error = json!({"code":self.code,"message":self.message,"request_id":id});
        if let Some(fields) = self.fields {
            error["fields"] = json!(fields);
        }
        let mut response = (self.status, axum::Json(json!({"error":error}))).into_response();
        if let Ok(value) = HeaderValue::from_str(&id) {
            response.headers_mut().insert("x-request-id", value);
        }
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        }
        if self.code == "KV5354" {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}
impl From<DatabaseError> for ApiError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::Validation(errors) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "KV5022",
                message: "字段校验失败".into(),
                fields: Some(errors.0),
            },
            DatabaseError::UnknownResource(_) => Self::missing(),
            DatabaseError::InvalidQuery(_) => Self::bad("查询参数不符合资源字段约束"),
            DatabaseError::UploadLimit => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "KV5129",
                "暂存文件已达到数量或总大小限制，请移除文件或稍后重试",
            ),
            DatabaseError::Conflict => Self::new(
                StatusCode::CONFLICT,
                "KV5009",
                "记录违反数据库约束，操作已回滚",
            ),
            DatabaseError::Busy | DatabaseError::Closed => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "KV5053",
                "数据库暂不可用，本次请求尚未执行",
            ),
            DatabaseError::WorkerLost => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "KV5051",
                "操作结果需要核对，请联系管理员确认，勿重复提交",
            ),
            _ => {
                tracing::error!(target: "kenvoy::http", "数据库请求失败，内部细节已隔离");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "KV5050",
                    "服务器无法完成请求",
                )
            }
        }
    }
}

fn request_id() -> String {
    format!(
        "request-{}",
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn header_once<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ApiError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(ApiError::bad("请求包含重复安全头"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| ApiError::bad("请求头不是有效文本"))
        })
        .transpose()
}

fn cookie(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let mut result = None;
    for value in headers.get_all(header::COOKIE) {
        let value = value
            .to_str()
            .map_err(|_| ApiError::bad("Cookie 格式无效"))?;
        for part in value.split(';') {
            let Some((name, value)) = part.trim().split_once('=') else {
                continue;
            };
            if name == COOKIE_NAME {
                if result.is_some() {
                    return Err(ApiError::bad("会话 Cookie 不能重复"));
                }
                if value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(ApiError::bad("会话 Cookie 格式无效"));
                }
                result = Some(value.to_owned());
            }
        }
    }
    Ok(result)
}

async fn identity(shared: &Shared, headers: &HeaderMap) -> Result<Option<SessionView>, ApiError> {
    match cookie(headers)? {
        Some(token) => Ok(shared.auth.authenticate(&token).await?),
        None => Ok(None),
    }
}
fn role(session: Option<&SessionView>) -> Role {
    session
        .and_then(|session| session.user.as_ref())
        .map(|user| user.role)
        .unwrap_or(Role::Anonymous)
}
fn permits(actual: Role, minimum: Role) -> bool {
    actual == Role::Admin || minimum == Role::Anonymous || actual == minimum
}
async fn require_write(shared: &Shared, headers: &HeaderMap) -> Result<SessionView, ApiError> {
    if header_once(headers, "origin")? != Some(shared.origin.as_str()) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "写请求必须来自配置的同源站点",
        ));
    }
    let token = cookie(headers)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "KV5001", "请先获取会话上下文"))?;
    let session = shared.auth.authenticate(&token).await?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "KV5001",
            "会话已过期，请重新获取会话上下文",
        )
    })?;
    let csrf = header_once(headers, "x-kenvoy-csrf")?
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "KV5013", "缺少 CSRF 校验值"))?;
    if !Auth::verify_csrf(&token, csrf) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "CSRF 校验失败",
        ));
    }
    Ok(session)
}
fn check_generation(headers: &HeaderMap, generation: u64) -> Result<(), ApiError> {
    if let Some(value) = header_once(headers, "x-kenvoy-generation")? {
        let value = value
            .parse::<u64>()
            .map_err(|_| ApiError::bad("应用代际编号无效"))?;
        if value != generation {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "KV5010",
                "应用已更新，请刷新页面描述后重试",
            ));
        }
    }
    Ok(())
}

async fn protect(State(shared): State<Arc<Shared>>, request: Request, next: Next) -> Response {
    let result = async {
        if header_once(request.headers(), "host")? != Some(shared.authority.as_str())
            || request
                .uri()
                .authority()
                .is_some_and(|value| value.as_str() != shared.authority)
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "KV5013",
                "请求主机不匹配 public_origin",
            ));
        }
        if header_once(request.headers(), "sec-fetch-site")? == Some("cross-site")
            && (request.uri().path() == "/_kenvoy/session"
                || !matches!(
                    *request.method(),
                    axum::http::Method::GET | axum::http::Method::HEAD
                ))
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "KV5013",
                "不接受跨站会话或写入请求",
            ));
        }
        cookie(request.headers())?;
        let upload = if request.method() == axum::http::Method::POST
            && request.uri().path().starts_with("/_kenvoy/uploads/")
        {
            let peer = request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|peer| peer.0);
            Some(uploads::prepare(&shared, request.headers(), request.uri(), peer).await?)
        } else {
            None
        };
        let body_limit = upload.as_ref().map_or(BODY_LIMIT, |upload| upload.max_size);
        let length = header_once(request.headers(), "content-length")?
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| ApiError::bad("Content-Length 无效"))
            })
            .transpose()?;
        if length.is_some_and(|value| value > body_limit) {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "KV5014",
                format!("请求体不能超过 {body_limit} 字节"),
            ));
        }
        let (mut parts, body) = request.into_parts();
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|peer| peer.0);
        parts.extensions.insert(ClientPeer(peer));
        if let Some(upload) = upload {
            parts.extensions.insert(upload);
        }
        let bytes = tokio::time::timeout(Duration::from_secs(30), to_bytes(body, body_limit))
            .await
            .map_err(|_| ApiError::new(StatusCode::REQUEST_TIMEOUT, "KV5008", "请求体接收超时"))?
            .map_err(|_| {
                ApiError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "KV5014",
                    "请求体超过限制或传输无效",
                )
            })?;
        Ok::<_, ApiError>(
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await,
        )
    }
    .await;
    let mut response = match result {
        Ok(response) => response,
        Err(error) => error.into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    if shared.development.read().await.is_some() {
        let scheme = if shared.secure_cookie { "wss" } else { "ws" };
        if let Ok(value) = HeaderValue::from_str(&format!(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: http: https:; connect-src 'self' {scheme}://{}; frame-ancestors 'none'; base-uri 'self'",
            shared.authority
        )) {
            headers.insert("content-security-policy", value);
        }
    } else {
        headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: http: https:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'"));
    }
    if !headers.contains_key("x-request-id")
        && let Ok(value) = HeaderValue::from_str(&request_id())
    {
        headers.insert("x-request-id", value);
    }
    response
}

#[cfg(test)]
mod tests;
