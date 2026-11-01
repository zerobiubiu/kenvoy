use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Anonymous,
    User,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Create,
    List,
    Read,
    Update,
    Delete,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum PermissionTarget {
    Resource(String),
    Action(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDefinition {
    pub target: PermissionTarget,
    pub operation: Operation,
    pub role: Role,
    /// 表单策略按单项匹配，不能把多个策略的字段并集合并授权。
    pub fields: Vec<String>,
    pub form: Option<String>,
}
