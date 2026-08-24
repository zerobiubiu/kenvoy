//! kengine-s3 — S3 对象存储抽象。
//!
//! 依据 `docs/04-storage.md`：项目只定义「如何连接和使用 S3」，不关心「S3 由谁提供」。
//! 更换存储服务只需改端点 / 凭证 / 区域 / Bucket 配置，无需改业务代码。

use s3::creds::Credentials;
use s3::region::Region;
use s3::{Bucket, BucketConfiguration};
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

/// S3 存储配置。
#[derive(Debug, Clone, Deserialize)]
pub struct S3Config {
    /// 存储服务端点（如 `http://127.0.0.1:9000`）。
    pub endpoint: String,
    /// 区域。
    pub region: String,
    /// 默认 Bucket。
    pub bucket: String,
    /// 访问密钥。
    pub access_key: String,
    /// 秘密密钥。
    pub secret_key: String,
}

/// 对象存储错误。
#[derive(Debug, Error)]
pub enum S3Error {
    /// S3 操作错误。
    #[error("S3 错误：{0}")]
    S3(#[from] s3::error::S3Error),
    /// 配置 / 凭据错误。
    #[error("{0}")]
    Other(String),
}

/// 对象存储封装（路径式访问，兼容 `MinIO` 等自托管 S3）。
#[derive(Debug, Clone)]
pub struct ObjectStore {
    bucket: Bucket,
    name: String,
    region: Region,
    credentials: Credentials,
}

impl ObjectStore {
    /// 依据配置连接对象存储。
    ///
    /// # Errors
    /// 配置无效时返回 [`S3Error`]。
    pub fn connect(cfg: &S3Config) -> Result<Self, S3Error> {
        let region = Region::Custom {
            region: cfg.region.clone(),
            endpoint: cfg.endpoint.clone(),
        };
        let credentials = Credentials::new(
            Some(&cfg.access_key),
            Some(&cfg.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| S3Error::Other(e.to_string()))?;
        let bucket =
            *Bucket::new(&cfg.bucket, region.clone(), credentials.clone())?.with_path_style();
        Ok(Self {
            bucket,
            name: cfg.bucket.clone(),
            region,
            credentials,
        })
    }

    /// 确保默认 Bucket 存在（不存在则创建）。
    ///
    /// # Errors
    /// 检查或创建失败时返回 [`S3Error`]。
    pub async fn ensure_bucket(&self) -> Result<(), S3Error> {
        if !self.bucket.exists().await? {
            Bucket::create_with_path_style(
                &self.name,
                self.region.clone(),
                self.credentials.clone(),
                BucketConfiguration::default(),
            )
            .await?;
        }
        Ok(())
    }

    /// 生成对象键（`uploads/<uuid>/<文件名>`）。
    #[must_use]
    pub fn object_key(file_name: &str) -> String {
        let uid = Uuid::new_v4();
        let safe = file_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        format!("uploads/{uid}/{safe}")
    }

    /// 上传对象，返回 `ETag`（响应头缺失时为 `None`）。
    ///
    /// # Errors
    /// 上传失败时返回 [`S3Error`]。
    pub async fn put(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<Option<String>, S3Error> {
        let resp = self
            .bucket
            .put_object_with_content_type(key, data, content_type)
            .await?;
        Ok(resp
            .headers()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("etag"))
            .map(|(_, v)| v.clone()))
    }

    /// 默认 Bucket 名称。
    #[must_use]
    pub fn bucket_name(&self) -> &str {
        &self.name
    }

    /// 下载对象。
    ///
    /// # Errors
    /// 下载失败（含 HTTP 非 2xx）时返回 [`S3Error`]。
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, S3Error> {
        let data = self.bucket.get_object(key).await?;
        if data.status_code() >= 300 {
            return Err(S3Error::Other(format!(
                "下载对象失败（HTTP {}）",
                data.status_code()
            )));
        }
        Ok(data.to_vec())
    }

    /// 删除对象。
    ///
    /// # Errors
    /// 删除失败时返回 [`S3Error`]。
    pub async fn delete(&self, key: &str) -> Result<(), S3Error> {
        self.bucket.delete_object(key).await?;
        Ok(())
    }
}
