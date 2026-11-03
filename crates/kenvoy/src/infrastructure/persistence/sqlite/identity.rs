//! 认证专用持久化命令；不接受任意 SQL 或业务资源名。
use super::{DatabaseError, Result};
use crate::{
    application::identity::{
        UserView,
        ports::{StoredSession, StoredUser},
    },
    domain::model::Role,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const SESSION_CAPACITY: i64 = 4096;
const USER_SESSION_CAPACITY: i64 = 32;
const USERS: &str = "CREATE TABLE _kenvoy_users(id INTEGER PRIMARY KEY AUTOINCREMENT CHECK(id BETWEEN 1 AND 9007199254740991),username TEXT NOT NULL UNIQUE,password_hash TEXT NOT NULL,role TEXT NOT NULL CHECK(role IN ('user','admin')),created_at INTEGER NOT NULL)";
const SESSIONS: &str = "CREATE TABLE _kenvoy_sessions(token_hash TEXT PRIMARY KEY NOT NULL CHECK(length(token_hash)=64 AND token_hash NOT GLOB '*[^0-9a-f]*'),user_id INTEGER REFERENCES _kenvoy_users(id) ON DELETE CASCADE,expires_at INTEGER NOT NULL,created_at INTEGER NOT NULL CHECK(expires_at>created_at))";
const META: &str = "CREATE TABLE _kenvoy_auth_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1))";
const EXPIRY: &str = "CREATE INDEX _kenvoy_sessions_expiry ON _kenvoy_sessions(expires_at)";
const USER_INDEX: &str = "CREATE INDEX _kenvoy_sessions_user ON _kenvoy_sessions(user_id)";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Session 无效或已经过期")]
    InvalidSession,
    #[error("管理员已经初始化，不能重复覆盖")]
    AlreadyInitialized,
    #[error("请先初始化管理员")]
    NotInitialized,
    #[error("用户名已经存在")]
    UsernameExists,
    #[error("Session 容量已满，请稍后再试")]
    SessionCapacity,
    #[error("认证数据库结构校验失败：{0}")]
    Schema(String),
}

type SchemaObject = (String, String, String, Option<String>);

fn snapshot(connection: &Connection) -> Result<Vec<SchemaObject>> {
    let mut stmt = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE tbl_name IN ('_kenvoy_users','_kenvoy_sessions','_kenvoy_auth_schema') ORDER BY type,name")?;
    Ok(stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?)
}

/// 迁移预检使用只读核验，不因缺少认证表而初始化或修复数据库。
pub(super) fn validate_schema(connection: &Connection) -> Result<()> {
    let actual = snapshot(connection)?;
    if actual.is_empty() {
        return Ok(());
    }
    let expected = Connection::open_in_memory()?;
    for sql in [USERS, SESSIONS, META, EXPIRY, USER_INDEX] {
        expected.execute_batch(sql)?;
    }
    if actual != snapshot(&expected)? {
        return Err(StoreError::Schema("固定认证表、索引或触发器发生漂移".into()).into());
    }
    let versions: Vec<(i64, i64)> = connection
        .prepare("SELECT id,version FROM _kenvoy_auth_schema")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    if versions != [(1, 1)] {
        return Err(StoreError::Schema("不支持的认证版本或版本记录损坏".into()).into());
    }
    if connection
        .prepare("PRAGMA foreign_key_check('_kenvoy_sessions')")?
        .exists([])?
    {
        return Err(StoreError::Schema("Session 存在失效的账号外键".into()).into());
    }
    Ok(())
}

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if snapshot(&transaction)?.is_empty() {
        for sql in [USERS, SESSIONS, META, EXPIRY, USER_INDEX] {
            transaction.execute_batch(sql)?;
        }
        transaction.execute(
            "INSERT INTO _kenvoy_auth_schema(id,version) VALUES(1,1)",
            [],
        )?;
    }
    validate_schema(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn role_name(role: Role) -> Result<&'static str> {
    match role {
        Role::Admin => Ok("admin"),
        Role::User => Ok("user"),
        Role::Anonymous => Err(DatabaseError::InvalidState("匿名身份不能持有账号".into())),
    }
}
fn decode_role(role: &str) -> Result<Role> {
    match role {
        "admin" => Ok(Role::Admin),
        "user" => Ok(Role::User),
        _ => Err(StoreError::Schema("账号角色无效".into()).into()),
    }
}
pub(crate) fn add_user(
    connection: &mut Connection,
    username: &str,
    password_hash: &str,
    role: Role,
    initial: bool,
    now: i64,
) -> Result<UserView> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM _kenvoy_users WHERE role='admin')",
        [],
        |row| row.get(0),
    )?;
    if initial && exists {
        return Err(StoreError::AlreadyInitialized.into());
    }
    if !initial && !exists {
        return Err(StoreError::NotInitialized.into());
    }
    if initial && role != Role::Admin {
        return Err(DatabaseError::InvalidState("初始账号必须为管理员".into()));
    }
    if transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM _kenvoy_users WHERE username=?1)",
        [username],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(StoreError::UsernameExists.into());
    }
    transaction.execute(
        "INSERT INTO _kenvoy_users(username,password_hash,role,created_at) VALUES(?1,?2,?3,?4)",
        params![username, password_hash, role_name(role)?, now],
    )?;
    let user = UserView {
        id: transaction.last_insert_rowid(),
        username: username.into(),
        role,
    };
    transaction.commit()?;
    Ok(user)
}
pub(crate) fn find_user(connection: &Connection, username: &str) -> Result<Option<StoredUser>> {
    let row = connection
        .query_row(
            "SELECT id,username,role,password_hash FROM _kenvoy_users WHERE username=?1",
            [username],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    row.map(|(id, username, role, password_hash)| {
        Ok(StoredUser {
            user: UserView {
                id,
                username,
                role: decode_role(&role)?,
            },
            password_hash,
        })
    })
    .transpose()
}
pub(crate) fn lookup(
    connection: &Connection,
    hash: &str,
    now: i64,
) -> Result<Option<StoredSession>> {
    let row=connection.query_row("SELECT s.expires_at,u.id,u.username,u.role,s.user_id FROM _kenvoy_sessions s LEFT JOIN _kenvoy_users u ON u.id=s.user_id WHERE s.token_hash=?1 AND s.expires_at>?2",params![hash,now],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<i64>>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<i64>>(4)?))).optional()?;
    row.map(|(expires_at, id, username, role, stored_id)| {
        Ok(StoredSession {
            expires_at,
            user: match (id, username, role, stored_id) {
                (Some(id), Some(username), Some(role), Some(expected)) if id == expected => {
                    Some(UserView {
                        id,
                        username,
                        role: decode_role(&role)?,
                    })
                }
                (None, None, None, None) => None,
                _ => return Err(StoreError::Schema("Session 账号引用损坏".into()).into()),
            },
        })
    })
    .transpose()
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
pub(crate) fn rotate(
    connection: &mut Connection,
    old_hash: Option<&str>,
    new_hash: &str,
    user_id: Option<i64>,
    now: i64,
    expires_at: i64,
) -> Result<StoredSession> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute("DELETE FROM _kenvoy_sessions WHERE token_hash IN (SELECT token_hash FROM _kenvoy_sessions WHERE expires_at<=?1 ORDER BY expires_at LIMIT 256)",[now])?;
    if let Some(old_hash) = old_hash {
        if lookup(&transaction, old_hash, now)?.is_none() {
            return Err(StoreError::InvalidSession.into());
        }
        transaction.execute(
            "DELETE FROM _kenvoy_sessions WHERE token_hash=?1",
            [old_hash],
        )?;
    }
    let total: i64 = transaction.query_row("SELECT count(*) FROM _kenvoy_sessions", [], |row| {
        row.get(0)
    })?;
    let user_total: i64 = transaction.query_row(
        "SELECT count(*) FROM _kenvoy_sessions WHERE user_id=?1 AND expires_at>?2",
        params![user_id, now],
        |row| row.get(0),
    )?;
    if total >= SESSION_CAPACITY || user_id.is_some() && user_total >= USER_SESSION_CAPACITY {
        return Err(StoreError::SessionCapacity.into());
    }
    transaction.execute("INSERT INTO _kenvoy_sessions(token_hash,user_id,expires_at,created_at) VALUES(?1,?2,?3,?4)",params![new_hash,user_id,expires_at,now])?;
    let session = lookup(&transaction, new_hash, now)?.ok_or(StoreError::InvalidSession)?;
    transaction.commit()?;
    Ok(session)
}
