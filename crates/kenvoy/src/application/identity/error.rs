use super::ports::CredentialError;
use crate::domain::identity::IdentityInputError;

#[derive(Debug, thiserror::Error)]
pub enum AuthError<E> {
    #[error("用户名或密码错误")]
    InvalidCredentials,
    #[error("Session 无效或已经过期，请重新获取登录上下文")]
    InvalidSession,
    #[error("认证输入无效：{0}")]
    InvalidInput(String),
    #[error("管理员已经初始化，不能重复覆盖")]
    AlreadyInitialized,
    #[error("请先初始化管理员")]
    NotInitialized,
    #[error("用户名已经存在")]
    UsernameExists,
    #[error("密码处理任务已满，请稍后再试")]
    Overloaded,
    #[error("Session 容量已满，请稍后再试")]
    SessionCapacity,
    #[error("认证密码学操作失败")]
    Crypto,
    #[error("系统时钟无法提供有效的认证期限")]
    Clock,
    #[error(transparent)]
    Database(E),
}

impl<E> From<IdentityInputError> for AuthError<E> {
    fn from(error: IdentityInputError) -> Self {
        Self::InvalidInput(error.0)
    }
}

impl<E> From<CredentialError> for AuthError<E> {
    fn from(error: CredentialError) -> Self {
        match error {
            CredentialError::Crypto => Self::Crypto,
            CredentialError::Clock => Self::Clock,
        }
    }
}
