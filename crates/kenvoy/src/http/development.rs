//! 开发状态与受控 Vite 代理；浏览器不能选择上游地址或取得内部代理密钥。
use super::{ApiError, ClientPeer, HttpRuntime, SetupError, Shared, header_once};
use axum::{
    Json,
    body::Body,
    extract::{
        FromRequestParts, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::{Semaphore, watch};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{self, client::IntoClientRequest, protocol::WebSocketConfig},
};

const PREFIX: &str = "/_kenvoy/vite/";
const SECRET_HEADER: &str = "x-kenvoy-vite-secret";
const MAX_RESPONSE: usize = 16 * 1024 * 1024;
const MAX_WS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentPhase {
    Ready,
    Building,
    Error,
}

/// secret 仅保留在 Rust 与受控前端工具之间，不实现 Debug/Serialize。
pub struct DevelopmentProxy {
    pub endpoint: SocketAddr,
    pub secret: String,
}

pub(super) struct Development {
    revision: u64,
    state: DevelopmentPhase,
    error: Option<String>,
    reload: bool,
    proxy: Option<Arc<Proxy>>,
}
struct Proxy {
    endpoint: SocketAddr,
    secret: HeaderValue,
    client: reqwest::Client,
    http: Semaphore,
    sockets: Arc<Semaphore>,
    stopped: watch::Sender<bool>,
}

impl Proxy {
    fn new(config: DevelopmentProxy) -> Result<Arc<Self>, SetupError> {
        if !config.endpoint.ip().is_loopback()
            || config.endpoint.port() == 0
            || config.secret.len() != 64
            || !config
                .secret
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(SetupError::Configuration(
                "开发代理必须为固定回环地址和 64 位十六进制内部密钥".into(),
            ));
        }
        let mut secret = HeaderValue::from_str(&config.secret)
            .map_err(|_| SetupError::Configuration("开发代理密钥格式无效".into()))?;
        secret.set_sensitive(true);
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(8)
            .build()
            .map_err(|_| SetupError::Configuration("无法创建开发代理 HTTP 客户端".into()))?;
        let (stopped, _) = watch::channel(false);
        Ok(Arc::new(Self {
            endpoint: config.endpoint,
            secret,
            client,
            http: Semaphore::new(32),
            sockets: Arc::new(Semaphore::new(MAX_WS)),
            stopped,
        }))
    }
    async fn stop(&self) -> Result<(), SetupError> {
        self.stopped.send_replace(true);
        let permits = tokio::time::timeout(
            Duration::from_secs(5),
            self.sockets.acquire_many(MAX_WS as u32),
        )
        .await
        .map_err(|_| SetupError::Publication("开发 WebSocket 未及时退出".into()))?
        .map_err(|_| SetupError::Publication("开发 WebSocket 队列已关闭".into()))?;
        drop(permits);
        Ok(())
    }
    async fn http(
        &self,
        method: Method,
        path: &str,
        headers: &HeaderMap,
        index: bool,
    ) -> Result<Response, ApiError> {
        if *self.stopped.borrow() {
            return Err(unavailable());
        }
        let _permit = self.http.try_acquire().map_err(|_| unavailable())?;
        let mut request = self
            .client
            .request(method.clone(), format!("http://{}{path}", self.endpoint))
            .header(SECRET_HEADER, self.secret.clone())
            .header(header::ACCEPT_ENCODING, "identity");
        // 只转发不含身份的有限缓存/格式协商头；不转发 Cookie、Authorization、Origin 或私有头。
        for name in [
            header::ACCEPT,
            header::IF_NONE_MATCH,
            header::IF_MODIFIED_SINCE,
        ] {
            if let Some(value) = headers.get(&name) {
                request = request.header(name, value);
            }
        }
        let mut upstream = request.send().await.map_err(|_| unavailable())?;
        let status = upstream.status();
        if status.is_client_error() {
            return Err(ApiError::missing());
        }
        if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
            return Err(unavailable());
        }
        if status.is_server_error() {
            return Err(unavailable());
        }
        if !index
            && upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.split(';').next() == Some("text/html"))
        {
            return Err(ApiError::missing());
        }
        if upstream
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE as u64)
        {
            return Err(unavailable());
        }
        let mut output = HeaderMap::new();
        for name in [header::CONTENT_TYPE, header::ETAG, header::LAST_MODIFIED] {
            if let Some(value) = upstream.headers().get(&name) {
                output.insert(name, value.clone());
            }
        }
        let mut body = Vec::new();
        while let Some(chunk) = upstream.chunk().await.map_err(|_| unavailable())? {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unavailable());
            }
            body.extend_from_slice(&chunk);
        }
        Ok((
            status,
            output,
            if method == Method::HEAD {
                Body::empty()
            } else {
                Body::from(body)
            },
        )
            .into_response())
    }
}

impl HttpRuntime {
    pub async fn enable_development(
        &self,
        proxy: Option<DevelopmentProxy>,
    ) -> Result<(), SetupError> {
        let proxy = proxy.map(Proxy::new).transpose()?;
        let mut development = self.shared.development.write().await;
        if development.is_some() {
            return Err(SetupError::Configuration("开发模式已经启用".into()));
        }
        *development = Some(Development {
            revision: 1,
            state: DevelopmentPhase::Ready,
            error: None,
            reload: false,
            proxy,
        });
        self.shared.development_changed.send_replace(1);
        Ok(())
    }
    pub async fn set_development_status(
        &self,
        state: DevelopmentPhase,
        error: Option<String>,
        reload: bool,
    ) -> Result<(), SetupError> {
        if error
            .as_ref()
            .is_some_and(|value| value.len() > 4096 || value.contains('\0'))
        {
            return Err(SetupError::Configuration(
                "开发错误摘要超过限制或包含 NUL".into(),
            ));
        }
        let mut development = self.shared.development.write().await;
        let development = development
            .as_mut()
            .ok_or_else(|| SetupError::Configuration("未启用开发模式".into()))?;
        let revision = development
            .revision
            .checked_add(1)
            .ok_or_else(|| SetupError::Publication("开发状态编号已耗尽".into()))?;
        development.revision = revision;
        development.state = state;
        development.error = error;
        development.reload = reload;
        self.shared.development_changed.send_replace(revision);
        Ok(())
    }
    pub async fn replace_development_proxy(
        &self,
        proxy: Option<DevelopmentProxy>,
    ) -> Result<(), SetupError> {
        let proxy = proxy.map(Proxy::new).transpose()?;
        let previous = {
            let mut development = self.shared.development.write().await;
            let development = development
                .as_mut()
                .ok_or_else(|| SetupError::Configuration("未启用开发模式".into()))?;
            std::mem::replace(&mut development.proxy, proxy)
        };
        if let Some(previous) = previous {
            previous.stop().await?;
        }
        Ok(())
    }
}

/// 等待时不持有模型或开发状态读锁，迁移和候选发布仍可取得写锁。
pub(super) async fn wait_ready(shared: &Shared, timeout: Duration) -> Result<(), ApiError> {
    let mut changed = shared.development_changed.subscribe();
    let ready = async {
        loop {
            let building = shared
                .development
                .read()
                .await
                .as_ref()
                .is_some_and(|state| matches!(state.state, DevelopmentPhase::Building));
            if !building {
                return Ok(());
            }
            changed.changed().await.map_err(|_| unavailable())?;
        }
    };
    tokio::time::timeout(timeout, ready).await.map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "KV5354",
            "应用更新尚未完成，请稍后重新打开页面",
        )
    })?
}

pub(super) async fn stop(shared: &Shared) -> Result<(), SetupError> {
    let proxy = shared
        .development
        .read()
        .await
        .as_ref()
        .and_then(|state| state.proxy.clone());
    if let Some(proxy) = proxy {
        proxy.stop().await?;
    }
    Ok(())
}

pub(super) async fn status(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    if uri.query().is_some() {
        return Err(ApiError::bad("开发状态不接受查询参数"));
    }
    if header_once(&headers, "sec-fetch-site")? == Some("cross-site") {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "不接受跨站开发状态请求",
        ));
    }
    let model = shared.model.read().await;
    let development = shared.development.read().await;
    let development = development.as_ref().ok_or_else(ApiError::missing)?;
    let mut value = json!({"revision":development.revision,"generation":model.generation,"state":development.state,"reload":development.reload});
    if let Some(error) = &development.error {
        value["error"] = json!(error);
    }
    Ok(Json(value))
}

async fn proxy(shared: &Shared) -> Result<Arc<Proxy>, ApiError> {
    shared
        .development
        .read()
        .await
        .as_ref()
        .and_then(|state| state.proxy.clone())
        .ok_or_else(ApiError::missing)
}

/// 只能在常规页面完成路径注册和身份检查后调用。
pub(super) async fn index(
    shared: &Shared,
    method: Method,
    headers: &HeaderMap,
) -> Option<Result<Response, ApiError>> {
    let proxy = shared
        .development
        .read()
        .await
        .as_ref()
        .and_then(|state| state.proxy.clone())?;
    Some(
        proxy
            .http(method, "/_kenvoy/vite/index.html", headers, true)
            .await,
    )
}

fn path(uri: &Uri) -> Result<&str, ApiError> {
    let raw = uri
        .path_and_query()
        .map(|value| value.as_str())
        .ok_or_else(ApiError::missing)?;
    let decoded = super::handlers::decode_uri(uri.path()).map_err(|_| ApiError::missing())?;
    if raw.len() > 8192
        || !uri.path().starts_with(PREFIX)
        || decoded.contains(['\\', '\0', '%'])
        || decoded
            .split('/')
            .any(|part| matches!(part, "." | ".." | "__open-in-editor" | "__inspect"))
        || uri
            .path()
            .as_bytes()
            .windows(3)
            .any(|code| code.eq_ignore_ascii_case(b"%2f") || code.eq_ignore_ascii_case(b"%5c"))
    {
        return Err(ApiError::missing());
    }
    if matches!(
        decoded.as_str(),
        "/_kenvoy/vite/" | "/_kenvoy/vite/index.html"
    ) {
        return Err(ApiError::missing());
    }
    Ok(raw)
}

pub(super) async fn assets(
    State(shared): State<Arc<Shared>>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, ApiError> {
    if !matches!(method, Method::GET | Method::HEAD) {
        return Err(ApiError::missing());
    }
    if header_once(&headers, "sec-fetch-site")? == Some("cross-site") {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "不接受跨站开发资源请求",
        ));
    }
    let proxy = proxy(&shared).await?;
    proxy.http(method, path(&uri)?, &headers, false).await
}

pub(super) async fn websocket(
    State(shared): State<Arc<Shared>>,
    request: Request,
) -> Result<Response, ApiError> {
    let proxy = proxy(&shared).await?;
    let peer = request
        .extensions()
        .get::<ClientPeer>()
        .copied()
        .unwrap_or(ClientPeer(None));
    let (mut parts, _body) = request.into_parts();
    let headers = &parts.headers;
    let uri = parts.uri.clone();
    if *proxy.stopped.borrow() {
        return Err(unavailable());
    }
    if header_once(headers, "origin")? != Some(shared.origin.as_str())
        || header_once(headers, "sec-websocket-protocol")? != Some("vite-hmr")
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "开发 WebSocket 必须使用同源 vite-hmr 协议",
        ));
    }
    super::handlers::limit_requests(
        &shared,
        [(format!("vite-ws:{}", super::handlers::peer_key(peer.0)), 60)],
    )?;
    let ws = WebSocketUpgrade::from_request_parts(&mut parts, &())
        .await
        .map_err(|_| ApiError::bad("开发 WebSocket 握手无效"))?;
    let permit = proxy
        .sockets
        .clone()
        .try_acquire_owned()
        .map_err(|_| unavailable())?;
    let mut request = format!("ws://{}{}", proxy.endpoint, path(&uri)?)
        .into_client_request()
        .map_err(|_| ApiError::bad("WebSocket URL 无效"))?;
    request
        .headers_mut()
        .insert(SECRET_HEADER, proxy.secret.clone());
    request.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static("vite-hmr"),
    );
    let config = WebSocketConfig::default()
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024))
        .max_write_buffer_size(2 * 1024 * 1024);
    let (upstream, response) = tokio::time::timeout(
        Duration::from_secs(3),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    if response
        .headers()
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        != Some("vite-hmr")
    {
        return Err(unavailable());
    }
    let stopped = proxy.stopped.subscribe();
    Ok(ws
        .protocols(["vite-hmr"])
        .max_message_size(1024 * 1024)
        .max_frame_size(1024 * 1024)
        .max_write_buffer_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            bridge(socket, upstream, stopped).await;
        }))
}

async fn bridge(
    mut browser: WebSocket,
    mut upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut stopped: watch::Receiver<bool>,
) {
    loop {
        if *stopped.borrow() {
            break;
        }
        tokio::select! {
            _=stopped.changed()=>break,
            message=browser.next()=>{
                let Some(Ok(message))=message else{break;};
                let message=match message{Message::Text(text)=>tungstenite::Message::Text(text.as_str().into()),Message::Binary(bytes)=>tungstenite::Message::Binary(bytes),Message::Ping(bytes)=>tungstenite::Message::Ping(bytes),Message::Pong(bytes)=>tungstenite::Message::Pong(bytes),Message::Close(_)=>break};
                if !matches!(tokio::time::timeout(Duration::from_secs(2),upstream.send(message)).await,Ok(Ok(()))){break;}
            },
            message=upstream.next()=>{
                let Some(Ok(message))=message else{break;};
                let message=match message{tungstenite::Message::Text(text)=>Message::Text(text.as_str().into()),tungstenite::Message::Binary(bytes)=>Message::Binary(bytes),tungstenite::Message::Ping(bytes)=>Message::Ping(bytes),tungstenite::Message::Pong(bytes)=>Message::Pong(bytes),tungstenite::Message::Close(_)=>break,tungstenite::Message::Frame(_)=>continue};
                if !matches!(tokio::time::timeout(Duration::from_secs(2),browser.send(message)).await,Ok(Ok(()))){break;}
            },
        }
    }
    let _ = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(200),
            browser.send(Message::Close(None))
        ),
        tokio::time::timeout(
            Duration::from_millis(200),
            upstream.send(tungstenite::Message::Close(None))
        )
    );
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "KV5353",
        "开发资源服务暂不可用，请等待构建恢复",
    )
}
