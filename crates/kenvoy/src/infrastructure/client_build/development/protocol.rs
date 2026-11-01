use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::DevClient;
use crate::infrastructure::client_build::{ClientBuildError, Result, error::invalid};

impl DevClient {
    pub(super) async fn command(&mut self, value: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        if bytes.len() > 4096 {
            return Err(invalid("开发控制指令超过限制"));
        }
        tokio::time::timeout(Duration::from_secs(20), self.input.write_all(&bytes))
            .await
            .map_err(|_| ClientBuildError::Timeout {
                operation: "开发控制写入".into(),
                seconds: 20,
            })?
            .map_err(|source| ClientBuildError::Spawn {
                operation: "开发控制写入".into(),
                source,
            })?;
        self.reply(self.sequence).await.map(|_| ())
    }

    pub(super) async fn reply(&mut self, id: u64) -> Result<Value> {
        let mut bytes = Vec::new();
        let count = tokio::time::timeout(
            Duration::from_secs(30),
            (&mut self.output).take(4097).read_until(b'\n', &mut bytes),
        )
        .await
        .map_err(|_| ClientBuildError::Timeout {
            operation: "开发控制响应".into(),
            seconds: 30,
        })?
        .map_err(|source| ClientBuildError::Spawn {
            operation: "开发控制响应".into(),
            source,
        })?;
        if count == 0 || count > 4096 || bytes.last() != Some(&b'\n') {
            return Err(invalid("开发服务关闭或返回无效控制响应"));
        }
        let response: Value = serde_json::from_slice(&bytes)?;
        if response["id"].as_u64() != Some(id) {
            return Err(invalid("开发控制响应序号不匹配"));
        }
        if response["ok"] != true {
            return Err(invalid(
                response["error"].as_str().unwrap_or("开发服务拒绝候选更新"),
            ));
        }
        Ok(response)
    }

    pub(super) async fn stop_process(&mut self) -> Result<()> {
        match tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(source)) => {
                return Err(ClientBuildError::Spawn {
                    operation: "等待 Vite 服务退出".into(),
                    source,
                });
            }
            Err(_) => {
                let _ = self.child.kill().await;
                self.child
                    .wait()
                    .await
                    .map_err(|source| ClientBuildError::Spawn {
                        operation: "回收 Vite 服务".into(),
                        source,
                    })?;
            }
        }
        if let Some(log) = self.log.take() {
            let bytes = log
                .await
                .map_err(|error| invalid(format!("开发日志任务失败：{error}")))?
                .map_err(|source| ClientBuildError::Spawn {
                    operation: "读取 Vite 日志".into(),
                    source,
                })?;
            let text = String::from_utf8_lossy(&bytes);
            if !text.trim().is_empty() {
                tracing::debug!(target:"kenvoy::build","{}",text.trim());
            }
        }
        self.closed = true;
        Ok(())
    }
}
