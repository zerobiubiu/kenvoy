use serde::{Deserialize, Serialize};

use crate::domain::source::SourceSpan;

use super::declarations::{ExpressionId, IdentifierIR, NodeId, StringLiteralIR};

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
