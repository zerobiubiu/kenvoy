use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Diagnostic, ModuleGraph, Severity, SourceSpan};

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

/// 模块内节点索引，仅在当前分析结果内有效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub usize);

/// 模块内表达式索引，不是跨修订稳定的符号标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExpressionId(pub usize);

/// 精确保留 JavaScript 字符串的 UTF-16 code units，包括孤立 surrogate。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringLiteralIR {
    pub span: SourceSpan,
    pub value_utf16: Vec<u16>,
}

impl StringLiteralIR {
    /// 仅当字符串可无损表示为 Rust UTF-8 时成功。
    pub fn to_string(&self) -> Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.value_utf16)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierIR {
    pub name: String,
    pub span: SourceSpan,
}

/// 标识符形式或字符串形式的导入导出名称，保留原始语法区别。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleNameIR {
    Identifier(IdentifierIR),
    String(StringLiteralIR),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSpecifierIR {
    pub span: SourceSpan,
    pub kind: ImportSpecifierKind,
    pub local: IdentifierIR,
    pub imported: Option<ModuleNameIR>,
    pub type_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportSpecifierKind {
    Default,
    Named,
    Namespace,
    TypeScriptEquals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportIR {
    pub span: SourceSpan,
    pub source: Option<StringLiteralIR>,
    pub specifiers: Vec<ImportSpecifierIR>,
    pub type_only: bool,
    pub attributes: Option<SourceSpan>,
    /// TypeScript 内部 namespace 别名不一定指向外部文件。
    pub typescript_module_reference: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSpecifierIR {
    pub span: SourceSpan,
    pub kind: ExportSpecifierKind,
    pub local: Option<ModuleNameIR>,
    pub exported: ModuleNameIR,
    pub type_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportSpecifierKind {
    Named,
    Namespace,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportKind {
    Named,
    All,
    Declaration {
        declaration_span: SourceSpan,
    },
    DefaultExpression {
        expression: ExpressionId,
    },
    DefaultDeclaration {
        declaration_span: SourceSpan,
        name: Option<IdentifierIR>,
    },
    TypeScriptAssignment {
        expression: ExpressionId,
    },
    TypeScriptNamespace {
        name: IdentifierIR,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportIR {
    pub span: SourceSpan,
    pub kind: ExportKind,
    pub source: Option<StringLiteralIR>,
    pub specifiers: Vec<ExportSpecifierIR>,
    pub type_only: bool,
    pub attributes: Option<SourceSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclarationKind {
    Function,
    Class,
    Const,
    Let,
    Var,
    Using,
    Interface,
    TypeAlias,
    Enum,
    Namespace,
}

/// 声明结构与模式中的绑定名称；求值和业务常量规则由调用方定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclarationIR {
    pub span: SourceSpan,
    pub kind: DeclarationKind,
    pub bindings: Vec<IdentifierIR>,
    /// 顶层 inline/default 声明导出语法标记，不是最终符号导出状态。
    /// export { name } 和 namespace 内导出需结合 ExportIR 及后续绑定分析。
    pub exported: bool,
    /// 逐项记录变量声明，避免把同一句声明中的多个绑定与初始值混淆。
    /// 旧版快照或非变量声明为空；空列表不表示已证明没有初始化副作用。
    #[serde(default)]
    pub declarators: Vec<VariableDeclaratorIR>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDeclaratorIR {
    pub span: SourceSpan,
    pub pattern: BindingPatternIR,
    pub initializer: Option<ExpressionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingPatternIR {
    pub span: SourceSpan,
    pub kind: BindingPatternKind,
}

/// 语法层面的绑定模式，不推断解构值，也不执行默认值表达式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingPatternKind {
    Identifier(IdentifierIR),
    Array {
        elements: Vec<Option<BindingPatternIR>>,
    },
    Object {
        properties: Vec<BindingPropertyIR>,
    },
    Rest {
        target: Box<BindingPatternIR>,
    },
    Assignment {
        target: Box<BindingPatternIR>,
        default: ExpressionId,
    },
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BindingPropertyIR {
    Property {
        span: SourceSpan,
        key: PropertyKeyIR,
        value: BindingPatternIR,
        shorthand: bool,
    },
    Rest {
        span: SourceSpan,
        target: BindingPatternIR,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiteralIR {
    String(StringLiteralIR),
    /// 非有限数值以 None 表示；精确字面形式始终由表达式 span 保留。
    Number {
        value: Option<f64>,
    },
    Boolean(bool),
    Null,
    BigInt {
        decimal: String,
    },
    Regex {
        pattern: String,
        flags: String,
    },
}

/// 不求值的表达式结构；direct_identifier 仅为语法文本，不是 Symbol 绑定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpressionIR {
    pub id: ExpressionId,
    pub span: SourceSpan,
    pub kind: String,
    pub direct_identifier: Option<IdentifierIR>,
    pub literal: Option<LiteralIR>,
    /// 与 jsx_roots 不同，本结构表达直接语法关系；不会将函数体 JSX 当作表达式结果。
    /// 旧版快照缺少结构时为 Opaque，调用方必须保守处理。
    #[serde(default)]
    pub shape: ExpressionShape,
    /// 表达式内最外层 JSX 出现位置，包含函数体中的 JSX，不执行函数。
    pub jsx_roots: Vec<NodeId>,
}

/// 保留静态分析所需的通用表达式结构，不执行表达式。
/// 未建模语法保留 Opaque、kind 和精确源码，不能据此假定它没有副作用。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum ExpressionShape {
    #[default]
    Opaque,
    Literal,
    Identifier,
    Jsx {
        node: NodeId,
    },
    Wrapper {
        kind: ExpressionWrapperKind,
        expression: ExpressionId,
    },
    Array {
        elements: Vec<Option<ExpressionItemIR>>,
    },
    Object {
        properties: Vec<ObjectPropertyIR>,
    },
    Unary {
        operator: String,
        argument: ExpressionId,
    },
    Binary {
        operator: String,
        left: ExpressionId,
        right: ExpressionId,
    },
    Conditional {
        test: ExpressionId,
        consequent: ExpressionId,
        alternate: ExpressionId,
    },
    Member {
        object: ExpressionId,
        property: MemberPropertyIR,
    },
    Template {
        quasis: Vec<TemplatePartIR>,
        expressions: Vec<ExpressionId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpressionWrapperKind {
    Parenthesized,
    TypeScriptAs,
    TypeScriptSatisfies,
    TypeScriptNonNull,
    TypeScriptTypeAssertion,
    TypeScriptConstAssertion,
    TypeScriptInstantiation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionItemIR {
    pub span: SourceSpan,
    pub expression: ExpressionId,
    pub spread: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropertyKeyIR {
    Identifier(IdentifierIR),
    String(StringLiteralIR),
    Number {
        span: SourceSpan,
        value: Option<f64>,
    },
    BigInt {
        span: SourceSpan,
        decimal: String,
    },
    Computed {
        span: SourceSpan,
        expression: ExpressionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberPropertyIR {
    Identifier(IdentifierIR),
    Private(IdentifierIR),
    Computed {
        span: SourceSpan,
        expression: ExpressionId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectPropertyIR {
    pub span: SourceSpan,
    pub kind: ObjectPropertyKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectPropertyKind {
    Value {
        key: PropertyKeyIR,
        value: ExpressionId,
        shorthand: bool,
    },
    Spread {
        expression: ExpressionId,
    },
    Method {
        key: PropertyKeyIR,
        kind: ObjectMethodKind,
    },
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectMethodKind {
    Method,
    Getter,
    Setter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplatePartIR {
    pub span: SourceSpan,
    pub raw: String,
    pub cooked_utf16: Option<Vec<u16>>,
    pub tail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementNameIR {
    Identifier(IdentifierIR),
    Member {
        parts: Vec<IdentifierIR>,
        span: SourceSpan,
    },
    Namespaced {
        namespace: IdentifierIR,
        name: IdentifierIR,
        span: SourceSpan,
    },
}

impl ElementNameIR {
    pub fn text(&self) -> String {
        match self {
            Self::Identifier(identifier) => identifier.name.clone(),
            Self::Member { parts, .. } => parts
                .iter()
                .map(|part| part.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
            Self::Namespaced {
                namespace, name, ..
            } => format!("{}:{}", namespace.name, name.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeValueIR {
    String(StringLiteralIR),
    Expression {
        expression: Option<ExpressionId>,
        span: SourceSpan,
    },
    Element(NodeId),
    Fragment(NodeId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeKind {
    /// value=None 表示裸布尔属性，不与显式 {true} 合并。
    Named {
        name: Box<ElementNameIR>,
        value: Option<AttributeValueIR>,
    },
    Spread {
        expression: ExpressionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeIR {
    pub span: SourceSpan,
    pub kind: AttributeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementIR {
    pub name: ElementNameIR,
    pub attributes: Vec<AttributeIR>,
    pub children: Vec<NodeId>,
    pub self_closing: bool,
    pub opening_span: SourceSpan,
    pub closing_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Element(Box<ElementIR>),
    Fragment { children: Vec<NodeId> },
    Text { value_utf16: Vec<u16> },
    Expression { expression: Option<ExpressionId> },
    Spread { expression: ExpressionId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeIR {
    pub id: NodeId,
    pub span: SourceSpan,
    pub kind: NodeKind,
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
    pub semantic: crate::SemanticIR,
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
    pub module_bindings: crate::ModuleBindingsIR,
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
