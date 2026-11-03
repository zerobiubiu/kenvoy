use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::{
    application::{
        execution::JsError,
        identity::AuthError,
        publication::ActionError,
        resources::{RepositoryFailure, ResourceError, ResourceRepository},
    },
    domain::{authorization::AuthorizationError, model::Role, validation::ValidationError},
};

use super::wiring::{HttpAuthError, Repository, RepositoryError};

pub(super) static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(super) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    fields: Option<Vec<ValidationError>>,
}
impl ApiError {
    pub(super) fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            fields: None,
        }
    }
    pub(super) fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "KV5000", message)
    }
    pub(super) fn missing() -> Self {
        Self::new(StatusCode::NOT_FOUND, "KV5004", "请求的资源不存在")
    }
    pub(super) fn denied(role: Role) -> Self {
        if role == Role::Anonymous {
            Self::new(StatusCode::UNAUTHORIZED, "KV5001", "请先登录")
        } else {
            Self::new(StatusCode::FORBIDDEN, "KV5003", "当前用户无权执行此操作")
        }
    }
    pub(super) fn action(name: &str, error: ActionError) -> Self {
        match error {
            ActionError::Authorization(error) => error.into(),
            ActionError::NotReady => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "KV6253",
                "函数运行时尚未准备完成",
            ),
            ActionError::Execution(error) => match error {
                JsError::UnknownFunction => Self::missing(),
                JsError::InvalidInput => Self::bad("函数参数超过允许范围"),
                JsError::Timeout => {
                    Self::new(StatusCode::GATEWAY_TIMEOUT, "KV6207", "函数执行超时")
                }
                JsError::Busy | JsError::Closed => Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "KV6253",
                    "函数运行时繁忙或正在关闭",
                ),
                _ => {
                    tracing::error!(target:"kenvoy::javascript",action=%name,"函数执行失败，内部异常未公开");
                    Self::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "KV6250",
                        "函数未能完成执行",
                    )
                }
            },
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let id = request_id();
        let mut error = json!({
            "code": self.code,
            "message": self.message,
            "message_key": self.code,
            "params": {},
            "request_id": id,
        });
        if let Some(fields) = self.fields {
            error["fields"] = json!(fields);
        }
        let mut response = (self.status, axum::Json(json!({"error":error}))).into_response();
        if let Ok(value) = HeaderValue::from_str(&id) {
            response.headers_mut().insert("x-request-id", value);
        }
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        }
        if self.code == "KV5354" {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}
impl From<AuthorizationError> for ApiError {
    fn from(error: AuthorizationError) -> Self {
        match error {
            AuthorizationError::UnknownResource => Self::missing(),
            AuthorizationError::Denied(role) => Self::denied(role),
        }
    }
}
impl From<ResourceError<RepositoryError>> for ApiError {
    fn from(error: ResourceError<RepositoryError>) -> Self {
        match error {
            ResourceError::Authorization(error) => error.into(),
            ResourceError::Repository(error) => {
                <Repository as ResourceRepository>::classify(error).into()
            }
            ResourceError::Missing => Self::missing(),
            ResourceError::UploadGenerationChanged => Self::new(
                StatusCode::CONFLICT,
                "KV5010",
                "上传期间应用已更新，请重新选择文件",
            ),
            ResourceError::UploadSessionExpired => Self::new(
                StatusCode::UNAUTHORIZED,
                "KV5001",
                "会话已过期，请重新选择文件",
            ),
            ResourceError::InvalidTime => {
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "KV5050", "系统时间无效")
            }
        }
    }
}
impl From<RepositoryFailure> for ApiError {
    fn from(error: RepositoryFailure) -> Self {
        match error {
            RepositoryFailure::Validation(errors) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "KV5022",
                message: "字段校验失败".into(),
                fields: Some(errors.0),
            },
            RepositoryFailure::UnknownResource => Self::missing(),
            RepositoryFailure::InvalidQuery => Self::bad("查询参数不符合资源字段约束"),
            RepositoryFailure::UploadLimit => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "KV5129",
                "暂存文件已达到数量或总大小限制，请移除文件或稍后重试",
            ),
            RepositoryFailure::Conflict => Self::new(
                StatusCode::CONFLICT,
                "KV5009",
                "记录违反数据库约束，操作已回滚",
            ),
            RepositoryFailure::Busy | RepositoryFailure::Closed => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "KV5053",
                "数据库暂不可用，本次请求尚未执行",
            ),
            RepositoryFailure::WorkerLost => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "KV5051",
                "操作结果需要核对，请联系管理员确认，勿重复提交",
            ),
            RepositoryFailure::Internal => {
                tracing::error!(target: "kenvoy::http", "数据库请求失败，内部细节已隔离");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "KV5050",
                    "服务器无法完成请求",
                )
            }
        }
    }
}
impl From<HttpAuthError> for ApiError {
    fn from(error: HttpAuthError) -> Self {
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
            AuthError::Database(error) => {
                <Repository as ResourceRepository>::classify(error).into()
            }
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

pub(super) fn request_id() -> String {
    format!(
        "request-{}",
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use axum::{
        body::to_bytes,
        http::{StatusCode, header},
        response::IntoResponse,
    };
    use serde_json::{Value, json};

    use crate::{
        application::resources::RepositoryFailure,
        domain::validation::{ValidationError, ValidationErrors},
    };

    use super::ApiError;

    #[tokio::test]
    async fn validation_metadata_reaches_http_without_changing_status_or_default_message()
    -> Result<(), Box<dyn std::error::Error>> {
        let field = ValidationError::new(
            "KV4005",
            Some("quantity".into()),
            "数字不满足 min/max 包含边界",
            "validation.number.min",
        )
        .with_param("min", 3);
        let response = ApiError::from(RepositoryFailure::Validation(ValidationErrors(vec![field])))
            .into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let request_id = response.headers()["x-request-id"].to_str()?.to_owned();
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        let error = &body["error"];
        assert_eq!(error["code"], "KV5022");
        assert_eq!(error["message_key"], "KV5022");
        assert_eq!(error["params"], json!({}));
        assert_eq!(error["request_id"], request_id);
        assert_eq!(error["fields"][0]["code"], "KV4005");
        assert_eq!(error["fields"][0]["field"], "quantity");
        assert_eq!(error["fields"][0]["message_key"], "validation.number.min");
        assert_eq!(error["fields"][0]["params"], json!({"min": 3}));
        Ok(())
    }

    #[tokio::test]
    async fn unknown_write_outcome_remains_distinct_from_unexecuted_busy_request()
    -> Result<(), Box<dyn std::error::Error>> {
        for (failure, status, code) in [
            (
                RepositoryFailure::WorkerLost,
                StatusCode::INTERNAL_SERVER_ERROR,
                "KV5051",
            ),
            (
                RepositoryFailure::Busy,
                StatusCode::SERVICE_UNAVAILABLE,
                "KV5053",
            ),
        ] {
            let response = ApiError::from(failure).into_response();
            assert_eq!(response.status(), status);
            assert!(response.headers().get(header::RETRY_AFTER).is_none());
            let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            assert_eq!(body["error"]["code"], code);
            assert_eq!(body["error"]["message_key"], code);
            assert_eq!(body["error"]["params"], json!({}));
        }
        Ok(())
    }
}
