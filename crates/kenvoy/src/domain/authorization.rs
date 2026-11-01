//! 资源、动作与选项写入的纯授权策略。

use std::fmt;

use serde_json::Value;

use crate::domain::model::{
    ActionDefinition, ApplicationIR, ExecutionEnvironment, Operation, PermissionDefinition,
    PermissionTarget, Role,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationError {
    UnknownResource,
    Denied(Role),
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnknownResource => "请求的资源不存在",
            Self::Denied(Role::Anonymous) => "请先登录",
            Self::Denied(_) => "当前用户无权执行此操作",
        };
        f.write_str(message)
    }
}

impl std::error::Error for AuthorizationError {}

pub fn permits(actual: Role, minimum: Role) -> bool {
    actual == Role::Admin || minimum == Role::Anonymous || actual == minimum
}

pub fn policy<'a>(
    app: &'a ApplicationIR,
    resource: &str,
    operation: Operation,
    role: Role,
    form: Option<&str>,
    fields: Option<&serde_json::Map<String, Value>>,
) -> Result<&'a PermissionDefinition, AuthorizationError> {
    app.resource(resource)
        .ok_or(AuthorizationError::UnknownResource)?;
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
        .ok_or(AuthorizationError::Denied(role))
}

pub fn authorize_option_write(
    app: &ApplicationIR,
    resource: &str,
    input: &Value,
    actual_role: Role,
    patch: bool,
) -> Result<(), AuthorizationError> {
    let resource = app
        .resource(resource)
        .ok_or(AuthorizationError::UnknownResource)?;
    for field in &resource.fields {
        if let Some(source) = &field.options_source
            && !permits(actual_role, source.permission)
            && (patch
                || input
                    .get(&field.name)
                    .or(field.default.as_ref())
                    .is_some_and(|value| !value.is_null()))
        {
            // PATCH 在事务内重验整条合并记录；先拒绝低权限调用，避免通过候选存在性探测私有数据。
            return Err(AuthorizationError::Denied(actual_role));
        }
    }
    Ok(())
}

pub fn action<'a>(
    app: &'a ApplicationIR,
    name: &str,
    role: Role,
) -> Result<&'a ActionDefinition, AuthorizationError> {
    let action = app
        .actions
        .iter()
        .find(|action| {
            action.name == name
                && matches!(
                    action.runtime,
                    ExecutionEnvironment::Server | ExecutionEnvironment::Shared
                )
        })
        .ok_or(AuthorizationError::UnknownResource)?;
    if !app.permissions.iter().any(|policy| {
        matches!(&policy.target,PermissionTarget::Action(target) if target==name)
            && policy.operation == Operation::Execute
            && permits(role, policy.role)
    }) {
        return Err(AuthorizationError::Denied(role));
    }
    Ok(action)
}
