//! 用户与权限 HTTP API（`/__kenvoy/auth/*`）。
//!
//! 覆盖：注册 / 登录 / 登出 / 当前用户、用户与角色管理（账号等级）、部门、通讯录
//! （含「人事新增人员但不需要账号」与账号关联）。

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use kengine_db::UserStore;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

/// auth 路由共享状态。
#[derive(Clone)]
pub struct AuthState {
    /// 数据库连接池（未配置时为 `None`）。
    pub db: Option<PgPool>,
}

/// 构建 auth 路由。
pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/users", get(list_users))
        .route("/users/{id}/roles", post(assign_role))
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

async fn user_detail_json(
    store: &UserStore<'_>,
    user: &kengine_db::users::User,
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
    let store = UserStore::new(pool);
    match store
        .create_user(&req.username, &req.password, &req.display_name)
        .await
    {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(err) => (StatusCode::CONFLICT, err.to_string()).into_response(),
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
                map.insert("token".to_string(), Value::String(token));
            }
            Json(detail).into_response()
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
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
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

async fn list_users(State(state): State<AuthState>) -> Response {
    let pool = match require_db(&state) {
        Ok(p) => p,
        Err(code) => return (code, "数据库未配置").into_response(),
    };
    match UserStore::new(pool).list_users().await {
        Ok(users) => Json(users).into_response(),
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
    Json(req): Json<AssignRoleReq>,
) -> Response {
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

async fn create_role(State(state): State<AuthState>, Json(req): Json<RoleReq>) -> Response {
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

async fn list_roles(State(state): State<AuthState>) -> Response {
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
    Json(req): Json<LevelReq>,
) -> Response {
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
    Json(req): Json<DepartmentReq>,
) -> Response {
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

async fn list_departments(State(state): State<AuthState>) -> Response {
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

async fn add_contact(State(state): State<AuthState>, Json(req): Json<ContactReq>) -> Response {
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
    axum::extract::Query(params): axum::extract::Query<ContactListParams>,
) -> Response {
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

/// 通讯录人员关联账号（人事 app 自动关联；`user_id` 为 `null` 时解除关联）。
async fn link_contact(
    State(state): State<AuthState>,
    Path(contact_id): Path<i64>,
    Json(req): Json<LinkUserReq>,
) -> Response {
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
