mod binding;

use serde::{Deserialize, Serialize};

use crate::domain::source::SourceSpan;

pub(crate) use binding::{function_scope, resolve_reference};

/// ID 仅对所属模块当前快照有效，不是持久化的跨修订标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SymbolId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReferenceId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    Module,
    FunctionParameters,
    Function,
    Block,
    Class,
    Catch,
    Loop,
    Namespace,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SymbolNamespace {
    Value,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Const,
    Let,
    Var,
    Using,
    Function,
    Class,
    Parameter,
    CatchParameter,
    Import,
    Interface,
    TypeAlias,
    TypeParameter,
    Enum,
    Namespace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    Read,
    Write,
    ReadWrite,
    Call,
    Construct,
    Component,
    Type,
    TypeQuery,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeIR {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBindingIR {
    pub source_utf16: Vec<u16>,
    /// default 对应默认导入；namespace 为 true 时不选择单个导出。
    pub imported: Option<String>,
    pub namespace: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIR {
    pub id: SymbolId,
    pub name: String,
    pub namespace: SymbolNamespace,
    pub kind: SymbolKind,
    pub scope: ScopeId,
    /// 合法重复 var、声明合并等共享词法绑定，保留每处声明位置。
    pub declarations: Vec<SourceSpan>,
    pub import: Option<ImportBindingIR>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceIR {
    pub id: ReferenceId,
    pub name: String,
    pub namespace: SymbolNamespace,
    pub kind: ReferenceKind,
    pub scope: ScopeId,
    pub span: SourceSpan,
    /// None 包含环境全局或无法静态绑定的名称，不自动断言为错误。
    pub symbol: Option<SymbolId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticIR {
    pub scopes: Vec<ScopeIR>,
    pub symbols: Vec<SymbolIR>,
    pub references: Vec<ReferenceIR>,
}

impl SemanticIR {
    pub fn scope(&self, id: ScopeId) -> Option<&ScopeIR> {
        self.scopes.get(id.0)
    }
    pub fn symbol(&self, id: SymbolId) -> Option<&SymbolIR> {
        self.symbols.get(id.0)
    }
    pub fn reference(&self, id: ReferenceId) -> Option<&ReferenceIR> {
        self.references.get(id.0)
    }
    /// 按标识符的精确字节范围查询；同一位置可分别包含值与类型引用。
    pub fn references_at(&self, start: usize, end: usize) -> impl Iterator<Item = &ReferenceIR> {
        self.references
            .iter()
            .filter(move |r| r.span.start == start && r.span.end == end)
    }
}
