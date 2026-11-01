use std::{path::Component, sync::Arc, time::Duration};

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    application::resources::{self, PageAccess},
    domain::model::ExecutionEnvironment,
};

use super::{ApiError, Shared, identity, request::decode_uri, role};

pub(super) async fn fallback(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    uri: Uri,
    method: Method,
) -> Result<Response, ApiError> {
    let raw_path = uri.path();
    if raw_path.as_bytes().windows(3).any(|part| {
        part.eq_ignore_ascii_case(b"%2f")
            || part.eq_ignore_ascii_case(b"%5c")
            || part.eq_ignore_ascii_case(b"%00")
    }) {
        return Err(ApiError::missing());
    }
    if raw_path.starts_with("/assets/") && raw_path.contains('%') {
        return Err(ApiError::missing());
    }
    let decoded = decode_uri(raw_path).map_err(|_| ApiError::missing())?;
    let path = decoded.as_str();
    if !matches!(method, Method::GET | Method::HEAD) {
        return Err(ApiError::missing());
    }
    if path.starts_with("/api/") || path.starts_with("/_kenvoy/") {
        return Err(ApiError::missing());
    }
    if !path.starts_with("/assets/") {
        super::development::wait_ready(&shared, Duration::from_secs(10)).await?;
    }
    let session = identity(&shared, &headers).await?;
    let role = role(session.as_ref());
    let model = shared.model.read().await;
    let asset = if let Some(file) = path.strip_prefix("/assets/") {
        if file.is_empty()
            || file.contains(['%', '\\', '\0'])
            || std::path::Path::new(file)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ApiError::missing());
        }
        format!("assets/{file}")
    } else {
        match resources::page(&model.application, path, role, model.assets.is_some())? {
            PageAccess::Index => "index.html".into(),
            PageAccess::Login => {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("next", path)
                    .finish();
                return Ok(Redirect::to(&format!("/admin/login?{query}")).into_response());
            }
        }
    };
    if asset == "index.html"
        && let Some(response) = super::development::index(&shared, method.clone(), &headers).await
    {
        return response;
    }
    let root = model
        .assets
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "KV5060", "前端资源尚未构建或接入"))?;
    super::wiring::serve_file(root, &asset, method, headers).await
}

pub(super) async fn method_not_allowed(State(shared): State<Arc<Shared>>, uri: Uri) -> Response {
    let path = match decode_uri(uri.path()) {
        Ok(path) => path,
        Err(error) => return error.into_response(),
    };
    let allow = if path == "/_kenvoy/development" || path.starts_with("/_kenvoy/vite/") {
        if shared.development.read().await.is_none() {
            return ApiError::missing().into_response();
        }
        "GET, HEAD"
    } else if let Some(name) = path.strip_prefix("/api/actions/") {
        if !shared
            .model
            .read()
            .await
            .application
            .actions
            .iter()
            .any(|action| {
                action.name == name
                    && matches!(
                        action.runtime,
                        ExecutionEnvironment::Server | ExecutionEnvironment::Shared
                    )
            })
        {
            return ApiError::missing().into_response();
        }
        "POST"
    } else if let Some(resource) = path.strip_prefix("/api/") {
        let mut parts = resource.split('/');
        let resource = parts.next().unwrap_or("");
        if shared
            .model
            .read()
            .await
            .application
            .resource(resource)
            .is_none()
        {
            return ApiError::missing().into_response();
        }
        if parts.next().is_some() {
            "GET, HEAD, PATCH, DELETE"
        } else {
            "GET, HEAD, POST"
        }
    } else if matches!(path.as_str(), "/_kenvoy/login" | "/_kenvoy/logout") {
        "POST"
    } else {
        "GET, HEAD"
    };
    let mut response = ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "KV5005",
        "该路由不支持此 HTTP 方法",
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}
