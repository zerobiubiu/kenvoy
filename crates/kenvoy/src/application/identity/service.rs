use super::{
    AuthError, SessionResponse, SessionView, UserView,
    ports::{CredentialProvider, IdentityRepository, StoredSession},
};
use crate::domain::{
    identity::{normalize_username, validate_password},
    model::Role,
};
use std::sync::Arc;
use tokio::sync::Semaphore;

const ANONYMOUS_TTL: i64 = 900;
const AUTHENTICATED_TTL: i64 = 28_800;

#[derive(Clone)]
pub struct Auth<R, C> {
    repository: R,
    provider: C,
    password_slots: Arc<Semaphore>,
    dummy_hash: Arc<String>,
}

impl<R: IdentityRepository, C: CredentialProvider> Auth<R, C> {
    pub async fn new(repository: R, provider: C) -> Result<Self, AuthError<R::Error>> {
        repository.initialize().await.map_err(R::classify_error)?;
        let dummy_password = hex(&provider.random::<32>()?);
        let credentials = provider.clone();
        let dummy_hash = tokio::task::spawn_blocking(move || {
            credentials.hash_password(dummy_password.as_bytes())
        })
        .await
        .map_err(|_| AuthError::Crypto)??;
        Ok(Self {
            repository,
            provider,
            password_slots: Arc::new(Semaphore::new(2)),
            dummy_hash: Arc::new(dummy_hash),
        })
    }
    pub async fn init_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserView, AuthError<R::Error>> {
        self.create_user(username, password, Role::Admin, true)
            .await
    }
    pub async fn add_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<UserView, AuthError<R::Error>> {
        self.create_user(username, password, role, false).await
    }
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
        initial: bool,
    ) -> Result<UserView, AuthError<R::Error>> {
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
        let provider = self.provider.clone();
        let hash = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            provider.hash_password(&password)
        })
        .await
        .map_err(|_| AuthError::Crypto)??;
        self.repository
            .add_user(username, hash, role, initial, self.provider.now()?)
            .await
            .map_err(R::classify_error)
    }
    pub async fn session(&self, raw: Option<&str>) -> Result<SessionResponse, AuthError<R::Error>> {
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
    pub async fn authenticate(
        &self,
        raw: &str,
    ) -> Result<Option<SessionView>, AuthError<R::Error>> {
        let Some(token) = decode_token(raw) else {
            return Ok(None);
        };
        let session = self
            .repository
            .lookup(hex(&self.provider.token_hash(&token)), self.provider.now()?)
            .await
            .map_err(R::classify_error)?;
        session.map(|session| self.view(raw, session)).transpose()
    }
    pub async fn login(
        &self,
        raw: &str,
        username: &str,
        password: &str,
    ) -> Result<SessionResponse, AuthError<R::Error>> {
        if self.authenticate(raw).await?.is_none() {
            return Err(AuthError::InvalidSession);
        }
        // 非法用户名与不存在账号都执行相同的固定参数哈希，避免直接暴露账号存在性。
        if password.len() > 128 {
            return Err(AuthError::InvalidCredentials);
        }
        let user = match normalize_username(username) {
            Ok(username) => self
                .repository
                .find_user(username)
                .await
                .map_err(R::classify_error)?,
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
        let provider = self.provider.clone();
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            provider.verify_password(&password, &hash)
        })
        .await
        .map_err(|_| AuthError::Crypto)??;
        let Some(user) = user.filter(|_| valid) else {
            return Err(AuthError::InvalidCredentials);
        };
        self.rotate(Some(raw), Some(user.user.id)).await
    }
    pub async fn logout(&self, raw: &str) -> Result<SessionResponse, AuthError<R::Error>> {
        self.rotate(Some(raw), None).await
    }
    async fn rotate(
        &self,
        old: Option<&str>,
        user_id: Option<i64>,
    ) -> Result<SessionResponse, AuthError<R::Error>> {
        let old_hash = old
            .map(|raw| {
                decode_token(raw)
                    .map(|token| hex(&self.provider.token_hash(&token)))
                    .ok_or(AuthError::InvalidSession)
            })
            .transpose()?;
        let token = self.provider.random::<32>()?;
        let raw = hex(&token);
        let now = self.provider.now()?;
        let expires = now
            .checked_add(if user_id.is_some() {
                AUTHENTICATED_TTL
            } else {
                ANONYMOUS_TTL
            })
            .ok_or(AuthError::Clock)?;
        let session = self
            .repository
            .rotate(
                old_hash,
                hex(&self.provider.token_hash(&token)),
                user_id,
                now,
                expires,
            )
            .await
            .map_err(R::classify_error)?;
        Ok(SessionResponse {
            view: self.view(&raw, session)?,
            set_token: Some(raw),
        })
    }
    pub fn verify_csrf(raw: &str, header: &str) -> bool {
        let (Some(token), Some(expected)) = (decode_token(raw), decode_token(header)) else {
            return false;
        };
        C::verify_csrf(&token, &expected)
    }
    fn view(&self, raw: &str, session: StoredSession) -> Result<SessionView, AuthError<R::Error>> {
        let token = decode_token(raw).ok_or(AuthError::InvalidSession)?;
        let csrf = self.provider.csrf_token(&token)?;
        Ok(SessionView {
            user: session.user,
            csrf_token: hex(&csrf),
            expires_at: session.expires_at,
        })
    }
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
#[path = "tests.rs"]
mod tests;
