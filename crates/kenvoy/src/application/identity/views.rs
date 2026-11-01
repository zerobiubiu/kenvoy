use crate::domain::model::Role;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserView {
    pub id: i64,
    pub username: String,
    pub role: Role,
}
#[derive(Clone, Serialize)]
pub struct SessionView {
    pub user: Option<UserView>,
    pub csrf_token: String,
    pub expires_at: i64,
}
// 不实现 Debug/Serialize，防止日志或 JSON 意外输出 Cookie 原文。
pub struct SessionResponse {
    pub view: SessionView,
    pub set_token: Option<String>,
}
