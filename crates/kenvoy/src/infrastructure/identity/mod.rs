//! 系统随机、Argon2 密码哈希与 Session 摘要的真实适配器。
use crate::application::identity::ports::{CredentialError, CredentialProvider};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version,
    password_hash::phc::PasswordHash,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const CSRF_LABEL: &[u8] = b"kenvoy.csrf.v1";
type Result<T, E = CredentialError> = std::result::Result<T, E>;

#[derive(Clone, Copy)]
pub struct SystemCredentials;

impl CredentialProvider for SystemCredentials {
    fn random<const N: usize>(&self) -> Result<[u8; N]> {
        random()
    }
    fn hash_password(&self, password: &[u8]) -> Result<String> {
        let salt = random::<16>()?;
        Ok(argon()?
            .hash_password_with_salt(password, &salt)
            .map_err(|_| CredentialError::Crypto)?
            .to_string())
    }
    fn verify_password(&self, password: &[u8], hash: &str) -> Result<bool> {
        // 持久化 PHC 同样是不可信输入，禁止其改变算法成本并耗尽内存。
        if hash.len() != 97 || !hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$") {
            return Err(CredentialError::Crypto);
        }
        let parsed = PasswordHash::new(hash).map_err(|_| CredentialError::Crypto)?;
        if parsed.salt.as_ref().is_none_or(|salt| salt.len() != 16)
            || parsed.hash.as_ref().is_none_or(|output| output.len() != 32)
        {
            return Err(CredentialError::Crypto);
        }
        Ok(argon()?.verify_password(password, &parsed).is_ok())
    }
    fn token_hash(&self, token: &[u8; 32]) -> [u8; 32] {
        Sha256::digest(token).into()
    }
    fn csrf_token(&self, token: &[u8; 32]) -> Result<[u8; 32]> {
        let mut mac = Hmac::<Sha256>::new_from_slice(token).map_err(|_| CredentialError::Crypto)?;
        mac.update(CSRF_LABEL);
        Ok(mac.finalize().into_bytes().into())
    }
    fn verify_csrf(token: &[u8; 32], expected: &[u8; 32]) -> bool {
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(token) else {
            return false;
        };
        mac.update(CSRF_LABEL);
        mac.verify_slice(expected).is_ok()
    }
    fn now(&self) -> Result<i64> {
        i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| CredentialError::Clock)?
                .as_secs(),
        )
        .map_err(|_| CredentialError::Clock)
    }
}

fn argon() -> Result<Argon2<'static>> {
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).map_err(|_| CredentialError::Crypto)?,
    ))
}
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| CredentialError::Crypto)?;
    Ok(bytes)
}
