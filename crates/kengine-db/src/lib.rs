//! kengine-db — PostgreSQL 数据层（基准 PG 18+）。
//!
//! 与 `kengine`（纯解析、无 I/O）分离：本 crate 承担全部持久化职责，
//! 供 CLI（`kenvoy`）与闭源侧（`KenvoySpace` / `Kenvoy-Runtime`）复用。

pub mod models;
pub mod schema;
pub mod storage;
pub mod users;

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

pub use models::ModelStore;
pub use schema::{SCHEMA_VERSION, migrate};
pub use storage::StorageStore;
pub use users::UserStore;

/// 数据库层错误。
#[derive(Debug, Error)]
pub enum DbError {
    /// SQL 执行错误。
    #[error("数据库错误：{0}")]
    Sql(#[from] sqlx::Error),
    /// 业务错误（含自定义信息）。
    #[error("{0}")]
    Other(String),
    /// 目标不存在。
    #[error("{0} 不存在")]
    NotFound(String),
}

/// 建立连接池。
///
/// # Errors
/// 连接失败时返回 [`DbError`]。
pub async fn connect(url: &str) -> Result<PgPool, DbError> {
    Ok(PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?)
}
