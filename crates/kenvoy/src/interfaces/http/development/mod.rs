//! 开发状态与受控 Vite 代理；浏览器不能选择上游地址或取得内部代理密钥。

mod proxy;
mod websocket;
pub(super) use websocket::websocket;

use super::{ApiError, HttpRuntime, SetupError, Shared, header_once};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::Response,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};

use proxy::Proxy;

const PREFIX: &str = "/_kenvoy/vite/";
const SECRET_HEADER: &str = "x-kenvoy-vite-secret";

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

async fn current_proxy(shared: &Shared) -> Result<Arc<Proxy>, ApiError> {
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
    let decoded = super::request::decode_uri(uri.path()).map_err(|_| ApiError::missing())?;
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
    let proxy = current_proxy(&shared).await?;
    proxy.http(method, path(&uri)?, &headers, false).await
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "KV5353",
        "开发资源服务暂不可用，请等待构建恢复",
    )
}
