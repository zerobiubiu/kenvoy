use crate::{
    application::identity::{
        AuthError, UserView,
        ports::{IdentityRepository, StoredSession, StoredUser},
    },
    domain::model::Role,
};

use super::super::{Database, DatabaseError, Result, identity::StoreError};

impl IdentityRepository for Database {
    type Error = DatabaseError;

    fn classify_error(error: Self::Error) -> AuthError<Self::Error> {
        match error {
            DatabaseError::Auth(StoreError::InvalidSession) => AuthError::InvalidSession,
            DatabaseError::Auth(StoreError::AlreadyInitialized) => AuthError::AlreadyInitialized,
            DatabaseError::Auth(StoreError::NotInitialized) => AuthError::NotInitialized,
            DatabaseError::Auth(StoreError::UsernameExists) => AuthError::UsernameExists,
            DatabaseError::Auth(StoreError::SessionCapacity) => AuthError::SessionCapacity,
            other => AuthError::Database(other),
        }
    }

    async fn initialize(&self) -> Result<()> {
        self.auth_initialize().await
    }

    async fn add_user(
        &self,
        username: String,
        password_hash: String,
        role: Role,
        initial: bool,
        now: i64,
    ) -> Result<UserView> {
        self.auth_add_user(username, password_hash, role, initial, now)
            .await
    }

    async fn find_user(&self, username: String) -> Result<Option<StoredUser>> {
        self.auth_find_user(username).await
    }

    async fn lookup(&self, hash: String, now: i64) -> Result<Option<StoredSession>> {
        self.auth_lookup(hash, now).await
    }

    async fn rotate(
        &self,
        old_hash: Option<String>,
        new_hash: String,
        user_id: Option<i64>,
        now: i64,
        expires_at: i64,
    ) -> Result<StoredSession> {
        self.auth_rotate(old_hash, new_hash, user_id, now, expires_at)
            .await
    }
}
