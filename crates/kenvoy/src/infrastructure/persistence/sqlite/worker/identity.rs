use crate::{
    application::identity::{
        UserView,
        ports::{StoredSession, StoredUser},
    },
    domain::model::Role,
};

use super::super::{Result, identity};
use super::Database;

impl Database {
    pub(crate) async fn auth_initialize(&self) -> Result<()> {
        self.submit(|state| identity::initialize(&mut state.connection))
            .await
    }
    pub(crate) async fn auth_add_user(
        &self,
        username: String,
        password_hash: String,
        role: Role,
        initial: bool,
        now: i64,
    ) -> Result<UserView> {
        self.submit(move |state| {
            identity::add_user(
                &mut state.connection,
                &username,
                &password_hash,
                role,
                initial,
                now,
            )
        })
        .await
    }
    pub(crate) async fn auth_find_user(&self, username: String) -> Result<Option<StoredUser>> {
        self.submit(move |state| identity::find_user(&state.connection, &username))
            .await
    }
    pub(crate) async fn auth_lookup(
        &self,
        hash: String,
        now: i64,
    ) -> Result<Option<StoredSession>> {
        self.submit(move |state| identity::lookup(&state.connection, &hash, now))
            .await
    }
    pub(crate) async fn auth_rotate(
        &self,
        old_hash: Option<String>,
        new_hash: String,
        user_id: Option<i64>,
        now: i64,
        expires_at: i64,
    ) -> Result<StoredSession> {
        self.submit(move |state| {
            identity::rotate(
                &mut state.connection,
                old_hash.as_deref(),
                &new_hash,
                user_id,
                now,
                expires_at,
            )
        })
        .await
    }
}
