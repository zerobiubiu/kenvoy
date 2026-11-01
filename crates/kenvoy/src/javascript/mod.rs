//! 可信项目的受限 JavaScript 适配；不提供 Node、文件、网络或数据库宿主能力。

pub(crate) mod compiler;
mod runtime;
pub(crate) mod source_slice;
pub use runtime::{JavascriptRuntime, JsError, Limits};

use crate::dsl::SourceLocation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use compiler::{prepare, validate};

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
