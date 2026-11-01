//! 通用 JS、TS、JSX、TSX 解析与模块分析引擎。
//!
//! 本库不解释 JSX 标签的业务含义，不执行用户代码，也不公开 SWC 内部类型。
//! 提供源码、通用表达式、模块依赖、作用域/绑定、增量会话和诊断，不做完整类型检查。

mod application;
mod domain;
mod infrastructure;
mod interfaces;

pub use application::error::{EngineError, ResolveError};
pub use application::incremental::{AnalysisStats, AnalysisUpdate};
pub use application::options::{AnalyzeOptions, ParseOptions, ResolveOptions};
pub use domain::diagnostics::{Diagnostic, DiagnosticCategory, RelatedLocation, Severity};
pub use domain::ir::*;
pub use domain::language::Language;
pub use domain::linker::{
    ExternalStarExportIR, LinkedExportIR, LinkedImportIR, ModuleBindingTarget, ModuleBindingsIR,
};
pub use domain::module::{DependencyKind, ModuleEdge, ModuleGraph, ResolvedModule};
pub use domain::semantic::{
    ImportBindingIR, ReferenceIR, ReferenceId, ReferenceKind, ScopeIR, ScopeId, ScopeKind,
    SemanticIR, SymbolIR, SymbolId, SymbolKind, SymbolNamespace,
};
pub use domain::source::{Position, SourceSpan};
pub use interfaces::{AnalysisSession, Engine};
