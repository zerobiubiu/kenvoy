use serde::{Deserialize, Serialize};

use crate::domain::source::SourceSpan;

use super::declarations::{ExpressionId, IdentifierIR, NodeId, StringLiteralIR};

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
