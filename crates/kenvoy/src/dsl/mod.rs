//! Kenvoy 业务声明分析；所有 JSX 标签含义均在本模块内定义。

mod analyzer;
pub mod static_eval;

use std::{fmt, path::PathBuf};

use kengine::SourceSpan;
use serde::{Deserialize, Serialize};

pub use analyzer::{AnalyzeResult, analyze};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DslDiagnosticCategory {
    StaticEvaluation,
    Semantic,
    Boundary,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DslDiagnostic {
    pub code: String,
    pub category: DslDiagnosticCategory,
    pub source: Box<SourceLocation>,
    pub message: String,
    pub suggestion: String,
    pub related: Vec<SourceLocation>,
}

impl DslDiagnostic {
    pub fn new(
        code: impl Into<String>,
        source: SourceLocation,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            category: DslDiagnosticCategory::StaticEvaluation,
            source: Box::new(source),
            message: message.into(),
            suggestion: suggestion.into(),
            related: vec![],
        }
    }
}

impl fmt::Display for DslDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}：{}\n文件：{}\n位置：{}:{}\n建议：{}",
            self.code,
            self.message,
            self.source.path.display(),
            self.source.span.start_position.line,
            self.source.span.start_position.column,
            self.suggestion
        )
    }
}

impl std::error::Error for DslDiagnostic {}
