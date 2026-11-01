mod declarations;
mod expressions;
mod jsx;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::{
    diagnostics::{Diagnostic, Severity},
    linker::ModuleBindingsIR,
    module::ModuleGraph,
    semantic::SemanticIR,
    source::SourceSpan,
};

pub use declarations::*;
pub use expressions::*;
pub use jsx::*;

/// Generic IR 的独立格式版本，不与 SWC 或应用业务模型共用版本号。
pub const GENERIC_IR_VERSION: u32 = 1;

fn deserialize_ir_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != GENERIC_IR_VERSION {
        return Err(serde::de::Error::custom(format!(
            "不支持 Generic IR 格式版本 {version}，当前仅支持 {GENERIC_IR_VERSION}"
        )));
    }
    Ok(version)
}

/// 模块内保存一份源码；所有重要结构的 raw 由 span 精确恢复。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleIR {
    #[serde(deserialize_with = "deserialize_ir_version")]
    pub format_version: u32,
    pub path: PathBuf,
    pub source: String,
    pub imports: Vec<ImportIR>,
    pub exports: Vec<ExportIR>,
    pub declarations: Vec<DeclarationIR>,
    pub nodes: Vec<NodeIR>,
    pub expressions: Vec<ExpressionIR>,
    pub jsx_roots: Vec<NodeId>,
    /// 模块内通用词法绑定；旧 v1 文档缺省为空，不伪造已经执行的分析。
    #[serde(default)]
    pub semantic: SemanticIR,
}

impl ModuleIR {
    pub fn raw(&self, span: &SourceSpan) -> Option<&str> {
        span.raw(&self.source)
    }
    pub fn node(&self, id: NodeId) -> Option<&NodeIR> {
        self.nodes.get(id.0)
    }
    pub fn expression(&self, id: ExpressionId) -> Option<&ExpressionIR> {
        self.expressions.get(id.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseOutput {
    pub module: Option<ModuleIR>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseOutput {
    pub fn get_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramIR {
    #[serde(deserialize_with = "deserialize_ir_version")]
    pub format_version: u32,
    pub modules: Vec<ModuleIR>,
    pub module_graph: ModuleGraph,
    /// 静态 ESM 导入与导出绑定；旧 v1 文档缺省为空。
    #[serde(default)]
    pub module_bindings: ModuleBindingsIR,
    pub diagnostics: Vec<Diagnostic>,
}

impl ProgramIR {
    pub fn get_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn get_module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == Severity::Error)
    }
}
