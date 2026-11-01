use super::{
    ApiError, AuthError, BODY_LIMIT, COOKIE_NAME, ClientPeer, Shared, check_generation, cookie,
    header_once, identity, permits, projection, require_write, role,
};
use crate::{
    application::{
        ApplicationIR, ExecutionEnvironment, Operation, PermissionDefinition, PermissionTarget,
        Role,
    },
    auth::SessionResponse,
    database::{FieldFilter, FilterOperator, ListQuery},
    validation,
};
use axum::{
    Extension, Json,
    body::{Bytes, to_bytes},
    extract::{ConnectInfo, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Component,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;
use tower_http::services::ServeFile;

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials => {
                Self::new(StatusCode::UNAUTHORIZED, "KV5101", "用户名或密码错误")
            }
            AuthError::InvalidSession => Self::new(
                StatusCode::UNAUTHORIZED,
                "KV5001",
                "会话已经失效，请重新获取会话上下文",
            ),
            AuthError::InvalidInput(_) => Self::bad("认证输入格式不正确"),
            AuthError::Overloaded | AuthError::SessionCapacity => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "KV5153",
                "认证服务繁忙，请稍后重试",
            ),
            AuthError::Database(error) => error.into(),
            _ => {
                tracing::error!(target:"kenvoy::http", "认证请求失败，内部细节已隔离");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "KV5150",
                    "认证服务无法完成请求",
                )
            }
        }
    }
}

fn no_query(uri: &Uri) -> Result<(), ApiError> {
    if uri.query().is_some_and(|query| !query.is_empty()) {
        Err(ApiError::bad("此接口不接受查询参数"))
    } else {
        Ok(())
    }
}
fn parse_json(headers: &HeaderMap, body: &[u8]) -> Result<Value, ApiError> {
    let content_type = header_once(headers, "content-type")?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "KV5015",
            "请求必须使用 application/json",
        )
    })?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "KV5015",
            "请求必须使用 application/json",
        ));
    }
    validation::parse_record_json(body)
        .map_err(|_| ApiError::bad("请求必须为有效且不含重复字段的 JSON 对象"))
}

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

pub(super) fn peer_key(peer: Option<SocketAddr>) -> String {
    peer.map(|peer| peer.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}
pub(super) fn limit_requests<const N: usize>(
    shared: &Shared,
    keys: [(String, u32); N],
) -> Result<(), ApiError> {
    let now = Instant::now();
    let mut limits = shared.request_limits.lock().map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "KV5153",
            "请求限制状态暂不可用",
        )
    })?;
    limits.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
    if keys
        .iter()
        .any(|(key, max)| limits.get(key).is_some_and(|(_, count)| count >= max))
        || limits.len()
            + keys
                .iter()
                .filter(|(key, _)| !limits.contains_key(key))
                .count()
            > 4096
    {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "KV5129",
            "请求过于频繁，请稍后重试",
        ));
    }
    for (key, _) in keys {
        limits.entry(key).or_insert((now, 0)).1 += 1;
    }
    Ok(())
}
fn limit_login(shared: &Shared, peer: Option<SocketAddr>, username: &str) -> Result<(), ApiError> {
    let hash = Sha256::digest(username.to_ascii_lowercase().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    limit_requests(
        shared,
        [
            (format!("login:{}", peer_key(peer)), 5),
            (format!("user:{hash}"), 5),
        ],
    )
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

pub(super) async fn application(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    let mut projection =
        projection::application(&model.application, role(session.as_ref()), model.generation);
    if shared.development.read().await.is_some() {
        projection["development"] = json!(true);
    }
    Ok(Json(projection))
}

pub(super) async fn action(
    State(shared): State<Arc<Shared>>,
    Path(name): Path<String>,
    Extension(peer): Extension<ClientPeer>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let action = model
        .application
        .actions
        .iter()
        .find(|action| {
            action.name == name
                && matches!(
                    action.runtime,
                    ExecutionEnvironment::Server | ExecutionEnvironment::Shared
                )
        })
        .ok_or_else(ApiError::missing)?;
    let role = role(Some(&session));
    if !model.application.permissions.iter().any(|policy| {
        matches!(&policy.target,PermissionTarget::Action(target) if target==&name)
            && policy.operation == Operation::Execute
            && permits(role, policy.role)
    }) {
        return Err(ApiError::denied(role));
    }
    let input = parse_json(&headers, &body)?;
    let object = input
        .as_object()
        .ok_or_else(|| ApiError::bad("Action 请求必须为 JSON 对象"))?;
    if object.len() != 1 {
        return Err(ApiError::bad("Action 只接受 args 字段"));
    }
    let args = object
        .get("args")
        .and_then(Value::as_array)
        .filter(|args| args.len() <= 32)
        .ok_or_else(|| ApiError::bad("args 必须为不超过 32 项的 JSON 数组"))?;
    let token = cookie(&headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    let hash = Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    limit_requests(
        &shared,
        [
            (format!("action:{}", peer_key(peer.0)), 60),
            (format!("action-session:{hash}"), 20),
        ],
    )?;
    let runtime = model.javascript.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "KV6253",
            "函数运行时尚未准备完成",
        )
    })?;
    let value=runtime.call(&action.handler,args.clone()).await.map_err(|error| {
        use crate::javascript::JsError;
        match error {
            JsError::UnknownFunction=>ApiError::missing(),
            JsError::InvalidInput=>ApiError::bad("函数参数超过允许范围"),
            JsError::Timeout=>ApiError::new(StatusCode::GATEWAY_TIMEOUT,"KV6207","函数执行超时"),
            JsError::Busy|JsError::Closed=>ApiError::new(StatusCode::SERVICE_UNAVAILABLE,"KV6253","函数运行时繁忙或正在关闭"),
            _=> {tracing::error!(target:"kenvoy::javascript",action=%name,"函数执行失败，内部异常未公开");ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"KV6250","函数未能完成执行")},
        }
    })?;
    Ok(Json(json!({"data":value,"generation":model.generation})))
}

fn policy<'a>(
    app: &'a ApplicationIR,
    resource: &str,
    operation: Operation,
    role: Role,
    form: Option<&str>,
    fields: Option<&serde_json::Map<String, Value>>,
) -> Result<&'a PermissionDefinition, ApiError> {
    app.resource(resource).ok_or_else(ApiError::missing)?;
    app.permissions
        .iter()
        .find(|policy| {
            matches!(&policy.target,PermissionTarget::Resource(name) if name==resource)
                && policy.operation == operation
                && permits(role, policy.role)
                && (policy.form.is_none() || policy.form.as_deref() == form)
                && fields.is_none_or(|fields| {
                    fields
                        .keys()
                        .all(|field| policy.fields.iter().any(|allowed| allowed == field))
                })
        })
        .ok_or_else(|| ApiError::denied(role))
}

fn id(value: &str) -> Result<i64, ApiError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::bad("记录 id 必须为正整数"));
    }
    let id = value
        .parse::<i64>()
        .map_err(|_| ApiError::bad("记录 id 超出允许范围"))?;
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(ApiError::bad("记录 id 超出允许范围"));
    }
    Ok(id)
}

fn list_query(uri: &Uri, app: &ApplicationIR, resource: &str) -> Result<ListQuery, ApiError> {
    let definition = app.resource(resource).ok_or_else(ApiError::missing)?;
    let raw = uri.query().unwrap_or("");
    if raw.len() > 8192 {
        return Err(ApiError::bad("查询参数过长"));
    }
    decode_uri(raw)?;
    // form_urlencoded 保留键值对顺序；先查重，不能先收集为 Map 后覆盖。
    let mut seen = BTreeSet::new();
    let mut query = ListQuery::default();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(key.to_string()) {
            return Err(ApiError::bad("查询参数不能重复"));
        }
        match key.as_ref() {
            "page" => {
                query.page = value
                    .parse()
                    .map_err(|_| ApiError::bad("page 必须为正整数"))?
            }
            "page_size" => {
                query.page_size = value
                    .parse()
                    .map_err(|_| ApiError::bad("page_size 必须为正整数"))?
            }
            "search" => query.search = Some(value.into_owned()),
            "sort" => query.sort = Some(value.into_owned()),
            "direction" => {
                query.descending = match value.as_ref() {
                    "asc" => false,
                    "desc" => true,
                    _ => return Err(ApiError::bad("direction 只能为 asc 或 desc")),
                }
            }
            key if key.starts_with("filter_") => {
                let field = &key[7..];
                if definition.field(field).is_none() {
                    return Err(ApiError::bad("筛选字段未在资源中声明"));
                }
                let value: Value = serde_json::from_str(&value).map_err(|_| {
                    ApiError::bad("筛选值必须为 JSON 标量，例如 25、true、null 或带引号的字符串")
                })?;
                if value.is_array() || value.is_object() {
                    return Err(ApiError::bad("筛选值必须为 JSON 标量"));
                }
                query.filters.push(FieldFilter {
                    field: field.into(),
                    operator: FilterOperator::Eq,
                    value,
                });
            }
            _ => return Err(ApiError::bad("存在不支持的查询参数")),
        }
    }
    Ok(query)
}

pub(super) async fn list(
    State(shared): State<Arc<Shared>>,
    Path(resource): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    policy(
        &model.application,
        &resource,
        Operation::List,
        role(session.as_ref()),
        None,
        None,
    )?;
    let query = list_query(&uri, &model.application, &resource)?;
    let result = shared.database.list(&resource, query).await?;
    Ok(Json(
        json!({"data":result.items,"page":result.page,"page_size":result.page_size,"total":result.total,"generation":model.generation}),
    ))
}

pub(super) async fn read(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    policy(
        &model.application,
        &resource,
        Operation::Read,
        role(session.as_ref()),
        None,
        None,
    )?;
    let record = shared
        .database
        .get(&resource, id(&record)?)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(json!({"data":record})))
}

pub(super) async fn create(
    State(shared): State<Arc<Shared>>,
    Extension(peer): Extension<ClientPeer>,
    Path(resource): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let role = role(Some(&session));
    let form = header_once(&headers, "x-kenvoy-form")?
        .map(decode_uri)
        .transpose()?;
    let policy = policy(
        &model.application,
        &resource,
        Operation::Create,
        role,
        form.as_deref(),
        input.as_object(),
    )?;
    if role != Role::Admin {
        let token = cookie(&headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
        let hash = Sha256::digest(token.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        limit_requests(
            &shared,
            [
                (format!("create:{}", peer_key(peer.0)), 60),
                (format!("create-session:{hash}"), 20),
            ],
        )?;
    }
    let mut record = shared.database.create(&resource, input).await?;
    if role != Role::Admin {
        record.retain(|key, _| {
            matches!(key.as_str(), "id" | "created_at" | "updated_at")
                || policy.fields.contains(key)
        });
    }
    Ok((StatusCode::CREATED, Json(json!({"data":record}))).into_response())
}

pub(super) async fn update(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    policy(
        &model.application,
        &resource,
        Operation::Update,
        role(Some(&session)),
        None,
        input.as_object(),
    )?;
    let record = shared
        .database
        .update(&resource, id(&record)?, input)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(json!({"data":record})))
}

pub(super) async fn delete(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    no_query(&uri)?;
    if !body.is_empty() {
        return Err(ApiError::bad("DELETE 不接受请求体"));
    }
    let session = require_write(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    policy(
        &model.application,
        &resource,
        Operation::Delete,
        role(Some(&session)),
        None,
        None,
    )?;
    if !shared.database.delete(&resource, id(&record)?).await? {
        return Err(ApiError::missing());
    }
    Ok(StatusCode::NO_CONTENT)
}

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
    } else if path == "/admin" || path == "/admin/login" || path.starts_with("/admin/") {
        if path != "/admin/login" {
            if path != "/admin"
                && !model
                    .application
                    .resources
                    .iter()
                    .any(|resource| path == format!("/admin/{}", resource.name))
            {
                return Err(ApiError::missing());
            }
            if role == Role::Anonymous && model.assets.is_some() {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("next", path)
                    .finish();
                return Ok(Redirect::to(&format!("/admin/login?{query}")).into_response());
            }
            if role != Role::Admin {
                return Err(ApiError::denied(role));
            }
        }
        "index.html".into()
    } else if let Some(page) = model
        .application
        .pages
        .iter()
        .find(|page| page.path == path)
    {
        if !permits(role, page.permission) {
            return Err(ApiError::denied(role));
        }
        "index.html".into()
    } else {
        return Err(ApiError::missing());
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

/// URL 解码前验证百分号及 UTF-8，禁止有损替换掩盖不同的参数或路由。
pub(super) fn decode_uri(raw: &str) -> Result<String, ApiError> {
    let mut decoded = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'%' {
            let code = bytes
                .get(offset + 1..offset + 3)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            let high = (code[0] as char)
                .to_digit(16)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            let low = (code[1] as char)
                .to_digit(16)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            decoded.push((high * 16 + low) as u8);
            offset += 3;
        } else {
            decoded.push(bytes[offset]);
            offset += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ApiError::bad("URL 必须为有效 UTF-8"))
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
