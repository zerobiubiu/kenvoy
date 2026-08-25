//! 用户与权限 HTTP API（`/__kenvoy/auth/*`），需要 `db` feature。
//!
//! 覆盖：注册（可配置：公开 / 审核 / 人事绑定式）、登录、登出、当前用户、
//! 用户与角色管理（账号等级）、部门、通讯录（含「人事新增人员但不需要账号」与账号关联）。
//! 管理端点（用户/角色/部门/通讯录、审核）要求登录；注册模式与密码强度由 `kenvoy.toml [auth]` 配置。

#![cfg(feature = "db")]

use crate::db::UserStore;
use crate::db::users::User;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Json, Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::config::{AuthConfig, MailConfig, password_policy};

/// auth 路由共享状态。
#[derive(Clone)]
pub struct AuthState {
    /// 数据库连接池（未配置时为 `None`）。
    pub db: Option<PgPool>,
    /// 注册模式与密码策略配置。
    pub auth: AuthConfig,
    /// 邮件配置（[mail] 段）。
    pub mail: MailConfig,
}

/// 构建 auth 路由。
///
/// 全局 body 兜底 64KB：auth 请求 body 都是 `{username, password, display_name, role, ...}`，
/// 远小于此值。足够大以容错，不致大 body 拖垮 server。
pub fn router() -> Router<AuthState> {
    Router::new()
        .layer(DefaultBodyLimit::max(64 * 1024))
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/forgot", post(forgot))
        .route("/reset", post(reset))
        .route("/me", get(me))
        .route("/options/users", get(list_user_options))
        .route("/options/departments", get(list_department_options))
        .route("/users", get(list_users))
        .route("/users/{id}/roles", post(assign_role))
        .route("/users/{id}/approve", post(approve_user))
        .route("/users/{id}/disable", post(disable_user))
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{id}/level", post(set_role_level))
        .route(
            "/departments",
            get(list_departments).post(create_department),
        )
        .route("/contacts", get(list_contacts).post(add_contact))
        .route("/contacts/{id}/link-user", post(link_contact))
}

fn require_db(state: &AuthState) -> Result<&PgPool, StatusCode> {
    state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// 读取 `kv_token` cookie（登录时种入；供 SSR 页面访问鉴权，Bearer 头优先）。
pub(crate) fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("kv_token=").map(str::to_string)
    })
}

/// 管理端点前置校验：数据库可用 + 已登录 + 账号等级达标（`[auth].admin_level`，缺省 90）。
async fn require_admin(
    state: &AuthState,
    headers: &HeaderMap,
) -> Result<User, (StatusCode, String)> {
    let pool = require_db(state).map_err(|c| (c, "数据库未配置".to_string()))?;
    let Some(token) = bearer_token(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "缺少 Bearer token".to_string()));
    };
    let store = UserStore::new(pool);
    let user = store
        .user_by_session(token)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "数据库错误".to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "会话无效或已过期".to_string()))?;
    let level = store
        .user_level(user.id)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "数据库错误".to_string()))?;
    if level < state.auth.admin_level {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "账号等级不足：管理操作需等级不低于 {}",
                state.auth.admin_level
            ),
        ));
    }
    Ok(user)
}

async fn user_detail_json(
    store: &UserStore<'_>,
    user: &crate::db::users::User,
) -> Result<Value, String> {
    let roles = store.user_roles(user.id).await.map_err(|e| e.to_string())?;
    let level = store.user_level(user.id).await.map_err(|e| e.to_string())?;
    Ok(json!({
        "user": user,
        "roles": roles.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
        "level": level,
    }))
}

#[derive(Deserialize)]
struct RegisterReq {
    username: String,
    password: String,
    display_name: String,
}

async fn register(State(state): State<AuthState>, Json(req): Json<RegisterReq>) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    // 密码强度校验（`[auth].password`，默认无校验）
    let policy = password_policy(&state.auth.password);
    let errors = policy.validate(&req.password, &req.username);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))).into_response();
    }
    let store = UserStore::new(pool);
    match state.auth.mode.as_str() {
        // 审核注册：账号进入 `pending`，审核通过后方可登录
        "review" => match store
            .create_user_with_status(&req.username, &req.password, &req.display_name, "pending")
            .await
        {
            Ok(id) => Json(
                json!({ "id": id, "status": "pending", "message": "注册已提交，等待管理员审核" }),
            )
            .into_response(),
            Err(err) => (StatusCode::CONFLICT, err.to_string()).into_response(),
        },
        // 人事绑定式：关闭公开注册，账号由人事/管理员创建后发放
        "hr" => (
            StatusCode::FORBIDDEN,
            "注册未开放：账号由人事应用创建后发放",
        )
            .into_response(),
        // 公开注册（默认）：成功后按 `[auth].default_role` 分配默认角色（新用户即可进入对应应用）
        _ => match store
            .create_user(&req.username, &req.password, &req.display_name)
            .await
        {
            Ok(id) => {
                if let Some(role_name) = state.auth.default_role.as_deref() {
                    match store.list_roles().await {
                        Ok(roles) => {
                            if let Some(role) = roles.into_iter().find(|r| r.name == role_name) {
                                if let Err(err) = store.assign_role(id, role.id).await {
                                    eprintln!("默认角色分配失败（分配）：{err}");
                                }
                            } else {
                                eprintln!("默认角色分配失败：角色不存在 {role_name}");
                            }
                        }
                        Err(err) => eprintln!("默认角色分配失败（查角色）：{err}"),
                    }
                }
                Json(json!({ "id": id })).into_response()
            }
            Err(err) => (StatusCode::CONFLICT, err.to_string()).into_response(),
        },
    }
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(State(state): State<AuthState>, Json(req): Json<LoginReq>) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    let store = UserStore::new(pool);
    match store.verify_login(&req.username, &req.password).await {
        Ok(Some(user)) => {
            let token = match store.create_session(user.id).await {
                Ok(t) => t,
                Err(err) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
                }
            };
            let mut detail = match user_detail_json(&store, &user).await {
                Ok(d) => d,
                Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
            };
            if let Value::Object(map) = &mut detail {
                map.insert("token".to_string(), Value::String(token.clone()));
            }
            // 种会话 cookie：供 SSR（页面访问权限）与同源导航使用；Bearer 头仍为首选
            let cookie = format!("kv_token={token}; Path=/; HttpOnly; SameSite=Lax");
            ([(axum::http::header::SET_COOKIE, cookie)], Json(detail)).into_response()
        }
        Ok(None) => (StatusCode::UNAUTHORIZED, "用户名或密码错误").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn logout(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    let Some(token) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "缺少 Bearer token").into_response();
    };
    match UserStore::new(pool).delete_session(token).await {
        Ok(()) => {
            let clear = "kv_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
            (
                [(axum::http::header::SET_COOKIE, clear)],
                StatusCode::NO_CONTENT,
            )
                .into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn me(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    let Some(token) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "缺少 Bearer token").into_response();
    };
    let store = UserStore::new(pool);
    match store.user_by_session(token).await {
        Ok(Some(user)) => match user_detail_json(&store, &user).await {
            Ok(detail) => Json(detail).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
        },
        Ok(None) => (StatusCode::UNAUTHORIZED, "会话无效或已过期").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn list_users(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).list_users().await {
        Ok(users) => Json(users).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 公共用户选项（`GET /__kenvoy/auth/options/users`）
///
/// 仅需登录（任意等级）：用于表单下拉选择，不要求 admin。
/// 返回 `[{id, username, displayName}]`。
async fn list_user_options(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    if bearer_token(&headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "缺少 Bearer token").into_response();
    }
    match sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, username, display_name FROM users WHERE status = 'active' ORDER BY display_name",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|(id, u, d)| serde_json::json!({"id": id, "username": u, "displayName": d}))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 公共部门选项（`GET /__kenvoy/auth/options/departments`）
///
/// 仅需登录。返回 `[{id, name, parentId}]`。
async fn list_department_options(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    if bearer_token(&headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "缺少 Bearer token").into_response();
    }
    match sqlx::query_as::<_, (i64, String, Option<i64>)>(
        "SELECT id, name, parent_id FROM departments ORDER BY sort_order, id",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|(id, n, p)| serde_json::json!({"id": id, "name": n, "parentId": p}))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct AssignRoleReq {
    role_id: i64,
}

async fn assign_role(
    State(state): State<AuthState>,
    Path(user_id): Path<i64>,
    headers: HeaderMap,
    Json(req): Json<AssignRoleReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).assign_role(user_id, req.role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct RoleReq {
    name: String,
    level: i32,
    description: Option<String>,
}

async fn create_role(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<RoleReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .create_role(&req.name, req.level, req.description.as_deref())
        .await
    {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(err) => (StatusCode::CONFLICT, err.to_string()).into_response(),
    }
}

async fn list_roles(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).list_roles().await {
        Ok(roles) => Json(roles).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct LevelReq {
    level: i32,
}

async fn set_role_level(
    State(state): State<AuthState>,
    Path(role_id): Path<i64>,
    headers: HeaderMap,
    Json(req): Json<LevelReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .set_role_level(role_id, req.level)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct DepartmentReq {
    name: String,
    parent_id: Option<i64>,
}

async fn create_department(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<DepartmentReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .create_department(&req.name, req.parent_id)
        .await
    {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn list_departments(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).list_departments().await {
        Ok(depts) => Json(depts).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct ContactReq {
    name: String,
    department_id: Option<i64>,
    user_id: Option<i64>,
    email: Option<String>,
    phone: Option<String>,
    title: Option<String>,
}

async fn add_contact(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<ContactReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .add_contact(
            &req.name,
            req.department_id,
            req.user_id,
            req.email.as_deref(),
            req.phone.as_deref(),
            req.title.as_deref(),
        )
        .await
    {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn list_contacts(
    State(state): State<AuthState>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<ContactListParams>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .list_contacts(params.keyword.as_deref())
        .await
    {
        Ok(contacts) => Json(contacts).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct ContactListParams {
    keyword: Option<String>,
}

#[derive(Deserialize)]
struct LinkUserReq {
    user_id: Option<i64>,
}

/// 审核通过用户（`pending` → `active`，审核注册模式）。
async fn approve_user(
    State(state): State<AuthState>,
    Path(user_id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).approve_user(user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::NOT_FOUND, err.to_string()).into_response(),
    }
}

/// 禁用账号（`active` → `disabled`，不得登录）。
async fn disable_user(
    State(state): State<AuthState>,
    Path(user_id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).disable_user(user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::NOT_FOUND, err.to_string()).into_response(),
    }
}
/// 通讯录人员关联账号（人事 app 自动关联；`user_id` 为 `null` 时解除关联）。
async fn link_contact(
    State(state): State<AuthState>,
    Path(contact_id): Path<i64>,
    headers: HeaderMap,
    Json(req): Json<LinkUserReq>,
) -> Response {
    if let Err((code, msg)) = require_admin(&state, &headers).await {
        return (code, msg).into_response();
    }
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool)
        .link_contact(contact_id, req.user_id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct ForgotReq {
    /// 用户名（重置发起的唯一标识）。
    username: String,
}

/// 申请重置密码：`POST /__kenvoy/auth/forgot` body `{"username":"..."}`。
///
/// 总是返回 200（不泄露账号是否存在）。生产环境应集成邮件发送链路发送重置链接；
/// 调试/无邮件后端时，响应体包含 `devToken` 供开发调试（带服务器日志告警）。
async fn forgot(State(state): State<AuthState>, Json(req): Json<ForgotReq>) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    let store = UserStore::new(pool);
    let Some(uid) = store
        .user_id_by_username(&req.username)
        .await
        .unwrap_or(None)
    else {
        // 不泄露用户存在性
        return Json(json!({ "ok": true })).into_response();
    };
    match store.create_reset_token(uid).await {
        Ok((_, plain)) => {
            // 发送重置邮件（配置缺失 / SMTP 错误仅 eprintln，不影响响应）
            // 邮箱优先用 users.email；未设置时无法发送（仅 eprintln 提示）
            if let Some(email) = store
                .user_email_by_id(uid)
                .await
                .ok()
                .flatten()
                .filter(|e| !e.is_empty())
            {
                let _ =
                    crate::mail::send_password_reset(&state.mail, &email, &req.username, &plain);
            } else {
                eprintln!(
                    "[kenvoy/mail] 用户 {} 无邮箱，无法发送密码重置邮件（token 已通过 eprintln 输出）",
                    req.username
                );
            }
            eprintln!(
                "[kenvoy] 密码重置 token 已生成（user_id={uid}），请部署邮件发送链路或将 token 发给用户：{plain}",
            );
            Json(json!({ "ok": true, "devToken": plain })).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct ResetReq {
    /// 重置链接中携带的明文 token。
    token: String,
    /// 新密码（明文，argon2 哈希入库）。
    new_password: String,
}

/// 重置密码：`POST /__kenvoy/auth/reset` body `{"token":"...","new_password":"..."}`。
async fn reset(State(state): State<AuthState>, Json(req): Json<ResetReq>) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    // 复用同一次 `password_policy` 校验
    let policy = password_policy(&state.auth.password);
    let errors = policy.validate(&req.new_password, "");
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))).into_response();
    }
    let store = UserStore::new(pool);
    let Some(uid) = store.consume_reset_token(&req.token).await.unwrap_or(None) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "errors": ["token 无效或已过期"] })),
        )
            .into_response();
    };
    match store.reset_password(uid, &req.new_password).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}
