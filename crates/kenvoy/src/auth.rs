//! 服务端身份与 Session；持久化层只接收摘要，不保存 Cookie 原文。
use crate::{
    application::Role,
    database::{
        Database, DatabaseError,
        auth::{StoreError, StoredSession},
    },
};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version,
    password_hash::phc::PasswordHash,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

const ANONYMOUS_TTL: i64 = 900;
const AUTHENTICATED_TTL: i64 = 28_800;
const CSRF_LABEL: &[u8] = b"kenvoy.csrf.v1";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
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
    Database(DatabaseError),
}
impl From<DatabaseError> for AuthError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::Auth(StoreError::InvalidSession) => Self::InvalidSession,
            DatabaseError::Auth(StoreError::AlreadyInitialized) => Self::AlreadyInitialized,
            DatabaseError::Auth(StoreError::NotInitialized) => Self::NotInitialized,
            DatabaseError::Auth(StoreError::UsernameExists) => Self::UsernameExists,
            DatabaseError::Auth(StoreError::SessionCapacity) => Self::SessionCapacity,
            other => Self::Database(other),
        }
    }
}
pub type Result<T> = std::result::Result<T, AuthError>;

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
#[derive(Clone)]
pub struct Auth {
    database: Database,
    password_slots: Arc<Semaphore>,
    dummy_hash: Arc<String>,
}

impl Auth {
    pub async fn new(database: Database) -> Result<Self> {
        database.auth_initialize().await?;
        let dummy_password = hex(&random::<32>()?);
        let dummy_hash =
            tokio::task::spawn_blocking(move || hash_password(dummy_password.as_bytes()))
                .await
                .map_err(|_| AuthError::Crypto)??;
        Ok(Self {
            database,
            password_slots: Arc::new(Semaphore::new(2)),
            dummy_hash: Arc::new(dummy_hash),
        })
    }
    pub async fn init_admin(&self, username: &str, password: &str) -> Result<UserView> {
        self.create_user(username, password, Role::Admin, true)
            .await
    }
    pub async fn add_user(&self, username: &str, password: &str, role: Role) -> Result<UserView> {
        self.create_user(username, password, role, false).await
    }
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
        initial: bool,
    ) -> Result<UserView> {
        let username = normalize_username(username)?;
        validate_password(password)?;
        if role == Role::Anonymous {
            return Err(AuthError::InvalidInput("匿名身份不能创建账号".into()));
        }
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Overloaded)?;
        let password = password.as_bytes().to_vec();
        let hash = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_password(&password)
        })
        .await
        .map_err(|_| AuthError::Crypto)??;
        Ok(self
            .database
            .auth_add_user(username, hash, role, initial, now()?)
            .await?)
    }
    pub async fn session(&self, raw: Option<&str>) -> Result<SessionResponse> {
        if let Some(raw) = raw
            && let Some(view) = self.authenticate(raw).await?
        {
            return Ok(SessionResponse {
                view,
                set_token: None,
            });
        }
        self.rotate(None, None).await
    }
    pub async fn authenticate(&self, raw: &str) -> Result<Option<SessionView>> {
        let Some(token) = decode_token(raw) else {
            return Ok(None);
        };
        let session = self
            .database
            .auth_lookup(hex(&Sha256::digest(token)), now()?)
            .await?;
        session.map(|session| view(raw, session)).transpose()
    }
    pub async fn login(
        &self,
        raw: &str,
        username: &str,
        password: &str,
    ) -> Result<SessionResponse> {
        if self.authenticate(raw).await?.is_none() {
            return Err(AuthError::InvalidSession);
        }
        // 非法用户名与不存在账号都执行相同的固定参数哈希，避免直接暴露账号存在性。
        if password.len() > 128 {
            return Err(AuthError::InvalidCredentials);
        }
        let user = match normalize_username(username) {
            Ok(username) => self.database.auth_find_user(username).await?,
            Err(_) => None,
        };
        let hash = user
            .as_ref()
            .map(|record| record.password_hash.clone())
            .unwrap_or_else(|| self.dummy_hash.as_ref().clone());
        let password = password.as_bytes().to_vec();
        let permit = self
            .password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Overloaded)?;
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verify_password(&password, &hash)
        })
        .await
        .map_err(|_| AuthError::Crypto)??;
        let Some(user) = user.filter(|_| valid) else {
            return Err(AuthError::InvalidCredentials);
        };
        self.rotate(Some(raw), Some(user.user.id)).await
    }
    pub async fn logout(&self, raw: &str) -> Result<SessionResponse> {
        self.rotate(Some(raw), None).await
    }
    async fn rotate(&self, old: Option<&str>, user_id: Option<i64>) -> Result<SessionResponse> {
        let old_hash = old
            .map(|raw| {
                decode_token(raw)
                    .map(|token| hex(&Sha256::digest(token)))
                    .ok_or(AuthError::InvalidSession)
            })
            .transpose()?;
        let token = random::<32>()?;
        let raw = hex(&token);
        let now = now()?;
        let expires = now
            .checked_add(if user_id.is_some() {
                AUTHENTICATED_TTL
            } else {
                ANONYMOUS_TTL
            })
            .ok_or(AuthError::Clock)?;
        let session = self
            .database
            .auth_rotate(old_hash, hex(&Sha256::digest(token)), user_id, now, expires)
            .await?;
        Ok(SessionResponse {
            view: view(&raw, session)?,
            set_token: Some(raw),
        })
    }
    pub fn verify_csrf(raw: &str, header: &str) -> bool {
        let (Some(token), Some(expected)) = (decode_token(raw), decode_token(header)) else {
            return false;
        };
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&token) else {
            return false;
        };
        mac.update(CSRF_LABEL);
        mac.verify_slice(&expected).is_ok()
    }
}
fn view(raw: &str, session: StoredSession) -> Result<SessionView> {
    let token = decode_token(raw).ok_or(AuthError::InvalidSession)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&token).map_err(|_| AuthError::Crypto)?;
    mac.update(CSRF_LABEL);
    Ok(SessionView {
        user: session.user,
        csrf_token: hex(&mac.finalize().into_bytes()),
        expires_at: session.expires_at,
    })
}
fn normalize_username(value: &str) -> Result<String> {
    if !(3..=64).contains(&value.len())
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.@-".contains(&b))
    {
        return Err(AuthError::InvalidInput("用户名须为 3 至 64 个 ASCII 字符，以字母或数字开头，仅允许字母、数字、下划线、点、@ 和连字符".into()));
    }
    Ok(value.to_ascii_lowercase())
}
fn validate_password(value: &str) -> Result<()> {
    if !(12..=128).contains(&value.len())
        || value.chars().any(char::is_control)
        || value.chars().all(char::is_whitespace)
    {
        return Err(AuthError::InvalidInput(
            "密码须为 12 至 128 个 UTF-8 字节，不能全为空白或含控制字符".into(),
        ));
    }
    Ok(())
}
fn argon() -> Result<Argon2<'static>> {
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).map_err(|_| AuthError::Crypto)?,
    ))
}
fn hash_password(password: &[u8]) -> Result<String> {
    let salt = random::<16>()?;
    Ok(argon()?
        .hash_password_with_salt(password, &salt)
        .map_err(|_| AuthError::Crypto)?
        .to_string())
}
fn verify_password(password: &[u8], hash: &str) -> Result<bool> {
    // 持久化 PHC 同样是不可信输入，禁止其改变算法成本并耗尽内存。
    if hash.len() != 97 || !hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$") {
        return Err(AuthError::Crypto);
    }
    let parsed = PasswordHash::new(hash).map_err(|_| AuthError::Crypto)?;
    if parsed.salt.as_ref().is_none_or(|salt| salt.len() != 16)
        || parsed.hash.as_ref().is_none_or(|output| output.len() != 32)
    {
        return Err(AuthError::Crypto);
    }
    Ok(argon()?.verify_password(password, &parsed).is_ok())
}
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Crypto)?;
    Ok(bytes)
}
fn now() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::Clock)?
            .as_secs(),
    )
    .map_err(|_| AuthError::Clock)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode_token(raw: &str) -> Option<[u8; 32]> {
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let digit = |b: u8| {
            if b.is_ascii_digit() {
                b - b'0'
            } else {
                b - b'a' + 10
            }
        };
        bytes[index] = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Some(bytes)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
