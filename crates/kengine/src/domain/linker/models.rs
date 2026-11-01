use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::{
    ir::ExpressionId,
    semantic::{SymbolId, SymbolNamespace},
    source::SourceSpan,
};

/// 导入绑定最终指向的通用语言实体；不会执行导出表达式或解析 namespace 成员。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleBindingTarget {
    Symbol {
        module: PathBuf,
        symbol: SymbolId,
    },
    Expression {
        module: PathBuf,
        expression: ExpressionId,
    },
    Declaration {
        module: PathBuf,
        span: SourceSpan,
    },
    Namespace {
        module: PathBuf,
    },
    /// external 由调用方处理，本结果不证明包确实导出了该名称。
    External {
        specifier: String,
        imported: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedExportIR {
    pub module: PathBuf,
    pub name: String,
    pub namespace: SymbolNamespace,
    /// 两个不同目标表示星号导出歧义；同一目标经多条路径转导不构成歧义。
    pub targets: Vec<ModuleBindingTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedImportIR {
    pub module: PathBuf,
    pub symbol: SymbolId,
    pub targets: Vec<ModuleBindingTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalStarExportIR {
    pub module: PathBuf,
    pub specifier: String,
    pub type_only: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleBindingsIR {
    pub exports: Vec<LinkedExportIR>,
    pub imports: Vec<LinkedImportIR>,
    /// 无法枚举 external 包的星号导出，显式保留边界，不假造完整导出集合。
    pub external_star_exports: Vec<ExternalStarExportIR>,
}
