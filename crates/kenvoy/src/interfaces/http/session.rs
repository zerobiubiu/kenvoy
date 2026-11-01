use std::{
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Extension, Json,
    body::{Bytes, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::{application::identity::SessionResponse, domain::model::Role};

use super::{
    ApiError, BODY_LIMIT, COOKIE_NAME, ClientPeer, Shared, cookie,
    request::{limit_login, limit_requests, no_query, parse_json, peer_key},
    require_write,
};

fn session_response(
    shared: &Shared,
    session: SessionResponse,
    generation: u64,
) -> Result<Response, ApiError> {
    let mut value = serde_json::to_value(&session.view).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "KV5150",
            "会话响应无法生成",
        )
    })?;
    value["generation"] = json!(generation);
    let mut response = Json(value).into_response();
    if let Some(token) = session.set_token {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "KV5150", "系统时间无效")
            })?
            .as_secs();
        let max_age = session
            .view
            .expires_at
            .saturating_sub(i64::try_from(now).unwrap_or(i64::MAX))
            .clamp(0, 28800);
        let cookie = format!(
            "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
            if shared.secure_cookie { "; Secure" } else { "" }
        );
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).map_err(|_| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "KV5150",
                    "会话 Cookie 无法生成",
                )
            })?,
        );
    }
    Ok(response)
}

pub(super) async fn session(
    State(shared): State<Arc<Shared>>,
    Extension(peer): Extension<ClientPeer>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, ApiError> {
    no_query(&uri)?;
    let token = cookie(&headers)?;
    let existing = if let Some(token) = &token {
        shared.auth.authenticate(token).await?
    } else {
        None
    };
    let session = if let Some(view) = existing {
        SessionResponse {
            view,
            set_token: None,
        }
    } else {
        limit_requests(
            &shared,
            [
                (format!("session:{}", peer_key(peer.0)), 20),
                ("session:global".into(), 120),
            ],
        )?;
        shared.auth.session(None).await?
    };
    session_response(&shared, session, shared.model.read().await.generation)
}

pub(super) async fn login(
    State(shared): State<Arc<Shared>>,
    request: Request,
) -> Result<Response, ApiError> {
    no_query(request.uri())?;
    require_write(&shared, request.headers()).await?;
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| peer.0);
    let token = cookie(request.headers())?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, BODY_LIMIT)
        .await
        .map_err(|_| ApiError::bad("请求体读取失败"))?;
    let input = parse_json(&parts.headers, &body)?;
    let object = input
        .as_object()
        .ok_or_else(|| ApiError::bad("登录请求必须为 JSON 对象"))?;
    if object.len() != 2
        || object
            .keys()
            .any(|key| !matches!(key.as_str(), "username" | "password"))
    {
        return Err(ApiError::bad("登录只接受 username 和 password"));
    }
    let username = object
        .get("username")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad("username 必须为字符串"))?;
    let password = object
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad("password 必须为字符串"))?;
    if username.len() > 128 || password.len() > 128 {
        return Err(ApiError::bad("登录输入超过允许长度"));
    }
    limit_login(&shared, peer, username)?;
    let session = shared.auth.login(&token, username, password).await?;
    session_response(&shared, session, shared.model.read().await.generation)
}

pub(super) async fn logout(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, ApiError> {
    no_query(&uri)?;
    require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    if !input.as_object().is_some_and(|object| object.is_empty()) {
        return Err(ApiError::bad("退出登录请求必须为空 JSON 对象"));
    }
    let token = cookie(&headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    let session = shared.auth.logout(&token).await?;
    session_response(&shared, session, shared.model.read().await.generation)
}
