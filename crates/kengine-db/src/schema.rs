//! 系统表结构（迁移）。
//!
//! 基准 PostgreSQL 18+。业务模型表由 [`crate::models`] 按 `IR` 动态创建。

use sqlx::PgPool;

use crate::DbError;

/// schema 版本。
pub const SCHEMA_VERSION: &str = "1.0";

/// 迁移语句（幂等：全部 `IF NOT EXISTS`）。
pub const MIGRATIONS: &[&str] = &[
    // 用户账号（账号等级 = 角色 level）
    r"
    CREATE TABLE IF NOT EXISTS users (
        id            BIGSERIAL PRIMARY KEY,
        username      TEXT NOT NULL UNIQUE,
        password_hash TEXT NOT NULL,
        display_name  TEXT NOT NULL,
        email         TEXT,
        phone         TEXT,
        status        TEXT NOT NULL DEFAULT 'active',
        created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    ",
    // 角色（level 越高权限越大，即「账号等级」）
    r"
    CREATE TABLE IF NOT EXISTS roles (
        id          BIGSERIAL PRIMARY KEY,
        name        TEXT NOT NULL UNIQUE,
        level       INT  NOT NULL DEFAULT 0,
        description TEXT
    )
    ",
    r"
    CREATE TABLE IF NOT EXISTS user_roles (
        user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        role_id BIGINT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, role_id)
    )
    ",
    // 部门（树形）
    r"
    CREATE TABLE IF NOT EXISTS departments (
        id         BIGSERIAL PRIMARY KEY,
        name       TEXT NOT NULL,
        parent_id  BIGINT REFERENCES departments(id) ON DELETE SET NULL,
        sort_order INT NOT NULL DEFAULT 0
    )
    ",
    // 通讯录：人员信息；user_id 可空 ——「人事新增人员但不需要账号」的场景
    r"
    CREATE TABLE IF NOT EXISTS contacts (
        id            BIGSERIAL PRIMARY KEY,
        name          TEXT NOT NULL,
        department_id BIGINT REFERENCES departments(id) ON DELETE SET NULL,
        user_id       BIGINT UNIQUE REFERENCES users(id) ON DELETE SET NULL,
        email         TEXT,
        phone         TEXT,
        title         TEXT,
        hire_date     DATE,
        status        TEXT NOT NULL DEFAULT 'active',
        created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    ",
    // 会话
    r"
    CREATE TABLE IF NOT EXISTS sessions (
        id         TEXT PRIMARY KEY,
        user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        expires_at TIMESTAMPTZ NOT NULL
    )
    ",
    // 文件元数据（文件本体在 S3，见 `docs/04-storage.md`）
    r"
    CREATE TABLE IF NOT EXISTS storage_object (
        id           TEXT PRIMARY KEY,
        bucket       TEXT NOT NULL,
        object_key   TEXT NOT NULL,
        file_name    TEXT NOT NULL,
        content_type TEXT,
        size_bytes   BIGINT NOT NULL DEFAULT 0,
        etag         TEXT,
        created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    ",
    // 审批流实例（运行时状态：当前节点与状态）
    r"
    CREATE TABLE IF NOT EXISTS kv_flow_instance (
        id           BIGSERIAL PRIMARY KEY,
        flow         TEXT NOT NULL,
        model        TEXT NOT NULL,
        record_id    BIGINT NOT NULL,
        current_node TEXT NOT NULL,
        status       TEXT NOT NULL DEFAULT 'running',
        created_by   BIGINT REFERENCES users(id) ON DELETE SET NULL,
        created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    ",
];

/// 执行全部迁移（幂等）。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn migrate(pool: &PgPool) -> Result<(), DbError> {
    for stmt in MIGRATIONS {
        sqlx::query(stmt).execute(pool).await?;
    }
    Ok(())
}
