use serde::{Deserialize, Serialize};

use crate::domain::source::SourceSpan;

use super::expressions::PropertyKeyIR;

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
