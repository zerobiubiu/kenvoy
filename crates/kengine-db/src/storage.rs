//! 存储对象元数据（文件本体在 S3，见 `docs/04-storage.md`）。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::DbError;

/// 存储对象元数据。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StorageObject {
    /// 文件 ID。
    pub id: String,
    /// S3 Bucket。
    pub bucket: String,
    /// S3 Object Key。
    pub object_key: String,
    /// 原始文件名。
    pub file_name: String,
    /// Content-Type。
    pub content_type: Option<String>,
    /// 文件大小（字节）。
    pub size_bytes: i64,
    /// `ETag`。
    pub etag: Option<String>,
}

/// 存储对象操作集合。
pub struct StorageStore<'a> {
    pool: &'a PgPool,
}

impl<'a> StorageStore<'a> {
    /// 创建存储对象存储。
    #[must_use]
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// 创建对象元数据记录，返回文件 ID。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn create(
        &self,
        bucket: &str,
        object_key: &str,
        file_name: &str,
        content_type: Option<&str>,
        size_bytes: i64,
        etag: Option<&str>,
    ) -> Result<String, DbError> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO storage_object (id, bucket, object_key, file_name, content_type, size_bytes, etag) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&id)
        .bind(bucket)
        .bind(object_key)
        .bind(file_name)
        .bind(content_type)
        .bind(size_bytes)
        .bind(etag)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// 按文件 ID 查询元数据。
    ///
    /// # Errors
    /// SQL 失败时返回 [`DbError`]。
    pub async fn get(&self, id: &str) -> Result<Option<StorageObject>, DbError> {
        Ok(sqlx::query_as::<_, StorageObject>(
            "SELECT id, bucket, object_key, file_name, content_type, size_bytes, etag \
             FROM storage_object WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?)
    }
}
