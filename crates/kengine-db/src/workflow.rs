//! 审批流实例存储（运行时状态：当前节点与状态）。
//!
//! 状态机执行逻辑在 `kengine::workflow`（纯函数）；本模块负责实例的持久化。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};

use crate::DbError;

/// 审批流实例。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FlowInstance {
    /// 实例 id。
    pub id: i64,
    /// 流程名（索引到 `ProjectIr.flows`）。
    pub flow: String,
    /// 关联模型名。
    pub model: String,
    /// 关联业务记录 id。
    pub record_id: i64,
    /// 当前节点名。
    pub current_node: String,
    /// 状态：`running` / `done` / `rejected`。
    pub status: String,
    /// 发起人用户 id。
    pub created_by: Option<i64>,
}

/// 流程实例操作集合。
pub struct FlowStore<'a> {
    pool: &'a PgPool,
}

impl<'a> FlowStore<'a> {
    /// 创建流程实例存储。
    #[must_use]
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// 创建流程实例，返回实例 id。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn create(
        &self,
        flow: &str,
        model: &str,
        record_id: i64,
        current_node: &str,
        created_by: Option<i64>,
    ) -> Result<i64, DbError> {
        let row = sqlx::query(
            "INSERT INTO kv_flow_instance (flow, model, record_id, current_node, created_by) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(flow)
        .bind(model)
        .bind(record_id)
        .bind(current_node)
        .bind(created_by)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }

    /// 按模型与记录查最近一个实例（无则 `None`）。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn latest_by_record(
        &self,
        model: &str,
        record_id: i64,
    ) -> Result<Option<FlowInstance>, DbError> {
        Ok(sqlx::query_as::<_, FlowInstance>(
            "SELECT id, flow, model, record_id, current_node, status, created_by \
             FROM kv_flow_instance WHERE model = $1 AND record_id = $2 \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(model)
        .bind(record_id)
        .fetch_optional(self.pool)
        .await?)
    }

    /// 更新实例当前节点与状态。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn advance(&self, id: i64, current_node: &str, status: &str) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE kv_flow_instance SET current_node = $1, status = $2, updated_at = now() \
             WHERE id = $3",
        )
        .bind(current_node)
        .bind(status)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
