use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::sync::{Semaphore, watch};

use super::{ApiError, DevelopmentProxy, SECRET_HEADER, SetupError, unavailable};

const MAX_RESPONSE: usize = 16 * 1024 * 1024;
const MAX_WS: usize = 64;

pub(super) struct Proxy {
    pub(super) endpoint: SocketAddr,
    pub(super) secret: HeaderValue,
    client: reqwest::Client,
    http: Semaphore,
    pub(super) sockets: Arc<Semaphore>,
    pub(super) stopped: watch::Sender<bool>,
}

impl Proxy {
    pub(super) fn new(config: DevelopmentProxy) -> Result<Arc<Self>, SetupError> {
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
    pub(super) async fn stop(&self) -> Result<(), SetupError> {
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
    pub(super) async fn http(
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
