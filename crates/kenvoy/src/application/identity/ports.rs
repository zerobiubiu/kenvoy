//! 认证用例实际使用的持久化与凭据端口。
use super::{AuthError, UserView};
use crate::domain::model::Role;

pub struct StoredUser {
    pub user: UserView,
    pub password_hash: String,
}
pub struct StoredSession {
    pub user: Option<UserView>,
    pub expires_at: i64,
}

pub trait IdentityRepository: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn initialize(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn add_user(
        &self,
        username: String,
        password_hash: String,
        role: Role,
        initial: bool,
        now: i64,
    ) -> impl Future<Output = Result<UserView, Self::Error>> + Send;
    fn find_user(
        &self,
        username: String,
    ) -> impl Future<Output = Result<Option<StoredUser>, Self::Error>> + Send;
    fn lookup(
        &self,
        hash: String,
        now: i64,
    ) -> impl Future<Output = Result<Option<StoredSession>, Self::Error>> + Send;
    fn rotate(
        &self,
        old_hash: Option<String>,
        new_hash: String,
        user_id: Option<i64>,
        now: i64,
        expires_at: i64,
    ) -> impl Future<Output = Result<StoredSession, Self::Error>> + Send;
    fn classify_error(error: Self::Error) -> AuthError<Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("认证密码学操作失败")]
    Crypto,
    #[error("系统时钟无法提供有效的认证期限")]
    Clock,
}

pub trait CredentialProvider: Clone + Send + Sync + 'static {
    fn random<const N: usize>(&self) -> Result<[u8; N], CredentialError>;
    fn hash_password(&self, password: &[u8]) -> Result<String, CredentialError>;
    fn verify_password(&self, password: &[u8], hash: &str) -> Result<bool, CredentialError>;
    fn token_hash(&self, token: &[u8; 32]) -> [u8; 32];
    fn csrf_token(&self, token: &[u8; 32]) -> Result<[u8; 32], CredentialError>;
    fn verify_csrf(token: &[u8; 32], expected: &[u8; 32]) -> bool;
    fn now(&self) -> Result<i64, CredentialError>;
}
