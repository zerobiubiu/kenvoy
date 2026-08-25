//! 用户 / 角色（账号等级）/ 通讯录 / 会话。
//!
//! 通讯录 `contacts.user_id` 可空：支持「人事办理新增人员但不需要账户」的场景。

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

use crate::db::DbError;

/// 用户账号。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    /// 用户 id。
    pub id: i64,
    /// 登录名。
    pub username: String,
    /// 显示名。
    pub display_name: String,
    /// 邮箱。
    pub email: Option<String>,
    /// 手机。
    pub phone: Option<String>,
    /// 状态。
    pub status: String,
}

/// 角色（账号等级）。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Role {
    /// 角色 id。
    pub id: i64,
    /// 角色名。
    pub name: String,
    /// 等级（越高权限越大）。
    pub level: i32,
    /// 描述。
    pub description: Option<String>,
}

/// 用户详情（含角色名与账号等级）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDetail {
    /// 用户账号。
    #[serde(flatten)]
    pub user: User,
    /// 角色名列表。
    pub roles: Vec<String>,
    /// 账号等级（最高角色等级）。
    pub level: i32,
}

/// 部门（树形）。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Department {
    /// 部门 id。
    pub id: i64,
    /// 部门名。
    pub name: String,
    /// 上级部门 id。
    pub parent_id: Option<i64>,
    /// 排序。
    pub sort_order: i32,
}

/// 通讯录人员（可无账号）。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Contact {
    /// 人员 id。
    pub id: i64,
    /// 姓名。
    pub name: String,
    /// 部门 id。
    pub department_id: Option<i64>,
    /// 关联账号 id（可空）。
    pub user_id: Option<i64>,
    /// 邮箱。
    pub email: Option<String>,
    /// 手机。
    pub phone: Option<String>,
    /// 职务。
    pub title: Option<String>,
    /// 状态。
    pub status: String,
}

/// 会话。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    /// 会话 token。
    pub id: String,
    /// 用户 id。
    pub user_id: i64,
    /// 过期时间。
    pub expires_at: chrono::DateTime<Utc>,
}

/// 用户存储操作集合。
pub struct UserStore<'a> {
    pool: &'a PgPool,
}

impl<'a> UserStore<'a> {
    /// 创建用户存储。
    #[must_use]
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// 创建用户（密码经 argon2 哈希），状态为 `active`。
    ///
    /// # Errors
    /// 用户名重复或 SQL 失败时返回 [`DbError`]。
    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        display_name: &str,
    ) -> Result<i64, DbError> {
        self.create_user_with_status(username, password, display_name, "active")
            .await
    }

    /// 创建用户并指定状态（如审核注册的 `pending`）。
    ///
    /// # Errors
    /// 用户名重复或 SQL 失败时返回 [`DbError`]。
    pub async fn create_user_with_status(
        &self,
        username: &str,
        password: &str,
        display_name: &str,
        status: &str,
    ) -> Result<i64, DbError> {
        let hash = hash_password(password)?;
        let row = sqlx::query(
            "INSERT INTO users (username, password_hash, display_name, status) \
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(username)
        .bind(&hash)
        .bind(display_name)
        .bind(status)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }

    /// 审核通过：`pending` → `active`（审核注册模式）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]；用户不存在时返回 [`DbError::NotFound`]。
    pub async fn approve_user(&self, user_id: i64) -> Result<(), DbError> {
        let row = sqlx::query("UPDATE users SET status = 'active' WHERE id = $1")
            .bind(user_id)
            .execute(self.pool)
            .await?;
        if row.rows_affected() == 0 {
            return Err(DbError::NotFound("用户不存在".to_string()));
        }
        Ok(())
    }

    /// 禁用账号（`active` → `disabled`，不得登录）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]；用户不存在时返回 [`DbError::NotFound`]。
    pub async fn disable_user(&self, user_id: i64) -> Result<(), DbError> {
        let row = sqlx::query("UPDATE users SET status = 'disabled' WHERE id = $1")
            .bind(user_id)
            .execute(self.pool)
            .await?;
        if row.rows_affected() == 0 {
            return Err(DbError::NotFound("用户不存在".to_string()));
        }
        Ok(())
    }

    /// 按登录名校验密码；成功返回用户。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn verify_login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, DbError> {
        let row = sqlx::query(
            "SELECT id, username, display_name, email, phone, status FROM users WHERE username = $1 AND status = 'active'",
        )
        .bind(username)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(row.get::<i64, _>("id"))
            .fetch_one(self.pool)
            .await?;
        if verify_password(password, &hash)? {
            Ok(Some(User {
                id: row.get("id"),
                username: row.get("username"),
                display_name: row.get("display_name"),
                email: row.get("email"),
                phone: row.get("phone"),
                status: row.get("status"),
            }))
        } else {
            Ok(None)
        }
    }

    /// 创建会话，返回 token。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn create_session(&self, user_id: i64) -> Result<String, DbError> {
        let token = Uuid::new_v4().to_string();
        let expires = Utc::now() + Duration::days(7);
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
            .bind(&token)
            .bind(user_id)
            .bind(expires)
            .execute(self.pool)
            .await?;
        Ok(token)
    }

    /// 按会话 token 查用户（未过期）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn user_by_session(&self, token: &str) -> Result<Option<User>, DbError> {
        let row = sqlx::query(
            "SELECT u.id, u.username, u.display_name, u.email, u.phone, u.status \
             FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.id = $1 AND s.expires_at > now()",
        )
        .bind(token)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| User {
            id: r.get("id"),
            username: r.get("username"),
            display_name: r.get("display_name"),
            email: r.get("email"),
            phone: r.get("phone"),
            status: r.get("status"),
        }))
    }

    /// 创建角色（账号等级）。
    ///
    /// # Errors
    /// 角色名重复或 SQL 失败时返回 [`DbError`]。
    pub async fn create_role(
        &self,
        name: &str,
        level: i32,
        description: Option<&str>,
    ) -> Result<i64, DbError> {
        let row = sqlx::query(
            "INSERT INTO roles (name, level, description) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(name)
        .bind(level)
        .bind(description)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get("id"))
    }

    /// 为角色分配账号等级。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn set_role_level(&self, role_id: i64, level: i32) -> Result<(), DbError> {
        sqlx::query("UPDATE roles SET level = $1 WHERE id = $2")
            .bind(level)
            .bind(role_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// 列出全部角色。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn list_roles(&self) -> Result<Vec<Role>, DbError> {
        Ok(sqlx::query_as::<_, Role>(
            "SELECT id, name, level, description FROM roles ORDER BY level DESC, id",
        )
        .fetch_all(self.pool)
        .await?)
    }

    /// 为用户分配角色。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn assign_role(&self, user_id: i64, role_id: i64) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(role_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 用户的最高角色等级（账号等级）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn user_level(&self, user_id: i64) -> Result<i32, DbError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(r.level), 0) AS level \
             FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = $1",
        )
        .bind(user_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get("level"))
    }

    /// 用户的全部角色。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn user_roles(&self, user_id: i64) -> Result<Vec<Role>, DbError> {
        Ok(sqlx::query_as::<_, Role>(
            "SELECT r.id, r.name, r.level, r.description \
             FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = $1",
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?)
    }

    /// 用户列表（含角色与账号等级）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn list_users(&self) -> Result<Vec<UserDetail>, DbError> {
        let users = sqlx::query_as::<_, User>(
            "SELECT id, username, display_name, email, phone, status FROM users ORDER BY id",
        )
        .fetch_all(self.pool)
        .await?;
        let mut out = Vec::with_capacity(users.len());
        for user in users {
            let roles = self.user_roles(user.id).await?;
            let level = self.user_level(user.id).await?;
            out.push(UserDetail {
                roles: roles.iter().map(|r| r.name.clone()).collect(),
                level,
                user,
            });
        }
        Ok(out)
    }

    /// 注销会话（登出）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn delete_session(&self, token: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(token)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// 创建密码重置 Token（明文 token 返回调用方发送邮件；DB 只存哈希）。
    ///
    /// 过期时间 1 小时。返回 `(token_id, plain_token)`。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn create_reset_token(&self, user_id: i64) -> Result<(i64, String), DbError> {
        use sha2::{Digest, Sha256};

        let plain = Uuid::new_v4().to_string() + &Uuid::new_v4().to_string();
        let mut hasher = Sha256::new();
        hasher.update(plain.as_bytes());
        let token_hash = format!("{:x}", hasher.finalize());
        let expires = Utc::now() + Duration::hours(1);
        let row = sqlx::query(
            "INSERT INTO kv_password_reset (user_id, token_hash, expires_at) \
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(&token_hash)
        .bind(expires)
        .fetch_one(self.pool)
        .await?;
        Ok((row.get::<i64, _>("id"), plain))
    }

    /// 按明文 token 查有效（未过期、未使用）的用户 id。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn consume_reset_token(&self, plain: &str) -> Result<Option<i64>, DbError> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(plain.as_bytes());
        let token_hash = format!("{:x}", hasher.finalize());
        let row: Option<(i64,)> = sqlx::query_as(
            "UPDATE kv_password_reset SET used_at = now() \
             WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() \
             RETURNING user_id",
        )
        .bind(&token_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|(u,)| u))
    }

    /// 管理员/CLI 按用户名查 id（重置流程需要 `user_id`）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn user_id_by_username(&self, username: &str) -> Result<Option<i64>, DbError> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(|(i,)| i))
    }

    /// 按 id 查邮箱（密码重置发送目标）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn user_email_by_id(&self, user_id: i64) -> Result<Option<String>, DbError> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT email FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.and_then(|(e,)| e))
    }

    /// 管理员/CLI 重置指定用户的密码（需传入新密码明文，函数内 argon2 哈希）。
    ///
    /// # Errors
    /// 用户不存在或 SQL 失败时返回 [`DbError`]。
    pub async fn reset_password(&self, user_id: i64, new_password: &str) -> Result<(), DbError> {
        let hash = hash_password(new_password)?;
        let row =
            sqlx::query("UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2")
                .bind(&hash)
                .bind(user_id)
                .execute(self.pool)
                .await?;
        if row.rows_affected() == 0 {
            return Err(DbError::NotFound("用户不存在".to_string()));
        }
        Ok(())
    }

    /// 列出全部部门。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn list_departments(&self) -> Result<Vec<Department>, DbError> {
        Ok(sqlx::query_as::<_, Department>(
            "SELECT id, name, parent_id, sort_order FROM departments ORDER BY sort_order, id",
        )
        .fetch_all(self.pool)
        .await?)
    }

    /// 通讯录人员关联账号（人事 app 自动关联；`user_id` 可空表示解除关联）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn link_contact(&self, contact_id: i64, user_id: Option<i64>) -> Result<(), DbError> {
        sqlx::query("UPDATE contacts SET user_id = $1 WHERE id = $2")
            .bind(user_id)
            .bind(contact_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// 创建部门（树形）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn create_department(
        &self,
        name: &str,
        parent_id: Option<i64>,
    ) -> Result<i64, DbError> {
        let row =
            sqlx::query("INSERT INTO departments (name, parent_id) VALUES ($1, $2) RETURNING id")
                .bind(name)
                .bind(parent_id)
                .fetch_one(self.pool)
                .await?;
        Ok(row.get("id"))
    }

    /// 新增通讯录人员；`user_id` 可空（无账号人员）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn add_contact(
        &self,
        name: &str,
        department_id: Option<i64>,
        user_id: Option<i64>,
        email: Option<&str>,
        phone: Option<&str>,
        title: Option<&str>,
    ) -> Result<i64, DbError> {
        let row = sqlx::query(
            "INSERT INTO contacts (name, department_id, user_id, email, phone, title) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(name)
        .bind(department_id)
        .bind(user_id)
        .bind(email)
        .bind(phone)
        .bind(title)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get("id"))
    }

    /// 列出通讯录人员（可带姓名过滤）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn list_contacts(&self, keyword: Option<&str>) -> Result<Vec<Contact>, DbError> {
        const BASE: &str =
            "SELECT id, name, department_id, user_id, email, phone, title, status FROM contacts";
        if let Some(kw) = keyword {
            Ok(
                sqlx::query_as::<_, Contact>(&format!("{BASE} WHERE name ILIKE $1 ORDER BY id"))
                    .bind(format!("%{kw}%"))
                    .fetch_all(self.pool)
                    .await?,
            )
        } else {
            Ok(sqlx::query_as::<_, Contact>(&format!("{BASE} ORDER BY id"))
                .fetch_all(self.pool)
                .await?)
        }
    }
}

/// argon2 密码哈希。
fn hash_password(password: &str) -> Result<String, DbError> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| DbError::Other(format!("密码哈希失败：{e}")))
}

/// 校验密码。
fn verify_password(password: &str, hash: &str) -> Result<bool, DbError> {
    let parsed =
        PasswordHash::new(hash).map_err(|e| DbError::Other(format!("无效的密码哈希：{e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
