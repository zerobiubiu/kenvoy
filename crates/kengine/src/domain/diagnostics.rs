use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::source::SourceSpan;

/// 诊断级别；含 Error 的分析结果不能作为有效编译结果使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticCategory {
    Syntax,
    Module,
    Io,
    Configuration,
}

/// 与主诊断关联的另一处源码位置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedLocation {
    pub path: PathBuf,
    pub span: Option<SourceSpan>,
    pub message: String,
}

/// 不依赖底层 Parser 的结构化诊断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub category: DiagnosticCategory,
    pub severity: Severity,
    pub path: PathBuf,
    pub span: Option<SourceSpan>,
    pub message: String,
    pub suggestion: String,
    pub related: Vec<RelatedLocation>,
}
