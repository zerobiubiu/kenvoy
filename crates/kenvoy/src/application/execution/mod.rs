//! 受限函数执行协议与端口，不包含 JavaScript 驱动。

use crate::domain::source::SourceLocation;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::{collections::BTreeMap, future::Future, time::Duration};

pub const BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Project,
    Untrusted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedBundle {
    pub format_version: u32,
    pub modules: BTreeMap<String, PreparedModule>,
    /// 键是 Application IR 的 handler 注册 ID，不接受任意函数名或源代码。
    pub functions: BTreeMap<String, FunctionTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedModule {
    pub code: String,
    pub imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FunctionTarget {
    pub module: String,
    pub export: String,
}

#[derive(Debug, Clone)]
pub struct JsDiagnostic {
    pub code: String,
    pub message: String,
    pub source: Option<Box<SourceLocation>>,
    pub suggestion: String,
}

impl std::fmt::Display for JsDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(source) = &self.source {
            write!(
                f,
                "{}:{}:{}：",
                source.path.display(),
                source.span.start_position.line,
                source.span.start_position.column
            )?;
        }
        write!(
            f,
            "{}：{}；建议：{}",
            self.code, self.message, self.suggestion
        )
    }
}
impl std::error::Error for JsDiagnostic {}

impl JsDiagnostic {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        source: Option<SourceLocation>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            source: source.map(Box::new),
            suggestion: suggestion.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub json_bytes: usize,
    pub arguments: usize,
    pub queue: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1),
            memory_bytes: 32 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            json_bytes: 1024 * 1024,
            arguments: 32,
            queue: 32,
        }
    }
}
impl Limits {
    pub(crate) fn validate(self) -> Result<Self, JsError> {
        if !(Duration::from_millis(10)..=Duration::from_secs(30)).contains(&self.timeout)
            || !(2 * 1024 * 1024..=128 * 1024 * 1024).contains(&self.memory_bytes)
            || !(64 * 1024..=1024 * 1024).contains(&self.stack_bytes)
            || !(1024..=1024 * 1024).contains(&self.json_bytes)
            || !(1..=32).contains(&self.arguments)
            || !(1..=64).contains(&self.queue)
        {
            return Err(JsError::Configuration);
        }
        Ok(self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JsError {
    #[error("KV6201：尚不支持在当前进程执行不可信 JavaScript")]
    Untrusted,
    #[error("KV6202：JavaScript 运行时限制配置无效")]
    Configuration,
    #[error("KV6203：JavaScript 函数不在已验证的注册表中")]
    UnknownFunction,
    #[error("KV6204：函数参数不符合 JSON 格式、数量或大小限制")]
    InvalidInput,
    #[error("KV6205：函数返回值不是受限 JSON 数据或超过大小限制")]
    InvalidOutput,
    #[error("KV6206：JavaScript 执行失败，异常细节未向调用方公开")]
    Execution,
    #[error("KV6207：JavaScript 执行超过时间限制")]
    Timeout,
    #[error("KV6208：JavaScript 内存不足")]
    Memory,
    #[error("KV6209：JavaScript 执行队列已满，此次调用未执行")]
    Busy,
    #[error("KV6210：JavaScript 运行时正在关闭，此次调用未执行")]
    Closed,
    #[error("KV6211：JavaScript 工作线程不可用")]
    WorkerLost,
    #[error(transparent)]
    Bundle(#[from] JsDiagnostic),
}

/// 调用已经验证、登记的函数，不接受任意函数名或源代码。
pub trait FunctionExecutor: Clone + Send + Sync + 'static {
    fn has_function(&self, handler: &str) -> bool;
    fn call(
        &self,
        handler: &str,
        args: Vec<Json>,
    ) -> impl Future<Output = Result<Json, JsError>> + Send;
}
