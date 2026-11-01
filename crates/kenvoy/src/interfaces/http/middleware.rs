use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{application::identity::SessionView, domain::model::Role};

use super::{
    ApiError, BODY_LIMIT, COOKIE_NAME, ClientPeer, Shared, errors::request_id, uploads,
    wiring::HttpAuth,
};

pub(super) fn header_once<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<Option<&'a str>, ApiError> {
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

pub(super) fn cookie(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
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

pub(super) async fn identity_with_token(
    shared: &Shared,
    headers: &HeaderMap,
) -> Result<(Option<SessionView>, Option<String>), ApiError> {
    let token = cookie(headers)?;
    let session = match token.as_deref() {
        Some(token) => shared.auth.authenticate(token).await?,
        None => None,
    };
    Ok((session, token))
}

pub(super) async fn identity(
    shared: &Shared,
    headers: &HeaderMap,
) -> Result<Option<SessionView>, ApiError> {
    Ok(identity_with_token(shared, headers).await?.0)
}
pub(super) fn role(session: Option<&SessionView>) -> Role {
    session
        .and_then(|session| session.user.as_ref())
        .map(|user| user.role)
        .unwrap_or(Role::Anonymous)
}
pub(super) async fn require_write(
    shared: &Shared,
    headers: &HeaderMap,
) -> Result<SessionView, ApiError> {
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
    if !HttpAuth::verify_csrf(&token, csrf) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "CSRF 校验失败",
        ));
    }
    Ok(session)
}
pub(super) fn check_generation(headers: &HeaderMap, generation: u64) -> Result<(), ApiError> {
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

pub(super) async fn protect(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
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
