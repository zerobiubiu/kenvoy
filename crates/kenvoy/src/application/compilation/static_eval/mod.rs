//! 声明专用的静态解释器，消费 KEngine 公共 IR 和语法适配端口，不执行 JavaScript。

mod bindings;
mod evaluator;
mod expressions;
mod jsx;
mod targets;
mod templates;
mod values;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use kengine::{ExpressionId, ProgramIR, SourceSpan, SymbolId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::source::SourceLocation;

pub use templates::{FunctionTemplate, FunctionTemplateParser};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedReference {
    pub module: PathBuf,
    pub export: String,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionReference {
    pub module: PathBuf,
    pub expression: ExpressionId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticValue {
    Json(Value),
    Reference(ResolvedReference),
    Expression(ExpressionReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticElement {
    pub name: String,
    pub attributes: BTreeMap<String, StaticValue>,
    pub children: Vec<StaticNode>,
    pub source: SourceLocation,
    pub module_ref: Option<ResolvedReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticNode {
    Element(StaticElement),
    Text {
        value: String,
        source: SourceLocation,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct EvalLimits {
    pub depth: usize,
    pub nodes: usize,
    pub collection: usize,
    pub steps: usize,
}

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            depth: 128,
            nodes: 20_000,
            collection: 10_000,
            steps: 100_000,
        }
    }
}

#[derive(Debug, Clone)]
enum Evaluated {
    Json(Value),
    Nodes(Vec<StaticNode>),
    Reference(ResolvedReference),
}

/// 每次入口求值使用独立预算和循环栈；错误结果不能用于生成 Schema。
pub struct Evaluator<'a> {
    program: &'a ProgramIR,
    template_parser: &'a dyn FunctionTemplateParser,
    limits: EvalLimits,
    steps: usize,
    nodes: usize,
    node_depth: usize,
    expressions: Vec<(PathBuf, ExpressionId)>,
    schemas: Vec<(PathBuf, String, SourceLocation)>,
    checked_modules: BTreeSet<PathBuf>,
    template_module: Option<&'a Path>,
    template_values: BTreeMap<SymbolId, templates::TemplateValue>,
}

#[cfg(test)]
mod tests;
