use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::domain::{
    diagnostics::{Diagnostic, DiagnosticCategory, Severity},
    ir::{ModuleIR, ModuleNameIR, ProgramIR, StringLiteralIR},
    module::ResolvedModule,
    semantic::{ScopeId, SymbolIR, SymbolId, SymbolNamespace},
    source::SourceSpan,
};

use super::{ExportTable, ModuleBindingTarget, Origin};

pub(super) fn namespaces(type_only: bool) -> Vec<SymbolNamespace> {
    if type_only {
        vec![SymbolNamespace::Type]
    } else {
        vec![SymbolNamespace::Value, SymbolNamespace::Type]
    }
}

pub(super) fn within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

pub(super) fn module_name(name: &ModuleNameIR) -> Option<String> {
    match name {
        ModuleNameIR::Identifier(identifier) => Some(identifier.name.clone()),
        ModuleNameIR::String(value) => value.to_string().ok(),
    }
}

pub(super) fn local_symbol(
    module: &ModuleIR,
    name: &str,
    namespace: SymbolNamespace,
) -> Option<SymbolId> {
    module
        .semantic
        .symbols
        .iter()
        .find(|symbol| {
            symbol.scope == ScopeId(0) && symbol.name == name && symbol.namespace == namespace
        })
        .map(|symbol| symbol.id)
}

pub(super) fn source_target(
    program: &ProgramIR,
    module: &Path,
    source: Option<&StringLiteralIR>,
) -> Option<ResolvedModule> {
    let source = source?;
    program
        .module_graph
        .edges
        .iter()
        .find(|edge| edge.importer == module && edge.span == source.span)
        .and_then(|edge| edge.target.clone())
}

pub(super) fn symbol_targets(
    program: &ProgramIR,
    table: &ExportTable,
    module: &ModuleIR,
    symbol: &SymbolIR,
    type_views: bool,
) -> Vec<ModuleBindingTarget> {
    let Some(binding) = &symbol.import else {
        return vec![ModuleBindingTarget::Symbol {
            module: module.path.clone(),
            symbol: symbol.id,
        }];
    };
    let Ok(specifier) = String::from_utf16(&binding.source_utf16) else {
        return vec![];
    };
    let target = program
        .module_graph
        .edges
        .iter()
        .find(|edge| edge.importer == module.path && edge.specifier == specifier)
        .and_then(|edge| edge.target.as_ref());
    match target {
        Some(ResolvedModule::File { path }) if binding.namespace => {
            vec![ModuleBindingTarget::Namespace {
                module: path.clone(),
            }]
        }
        Some(ResolvedModule::File { path }) => binding
            .imported
            .as_ref()
            .and_then(|name| exported_targets(table, path, name, symbol.namespace, type_views))
            .unwrap_or_default(),
        Some(ResolvedModule::External { specifier }) => vec![ModuleBindingTarget::External {
            specifier: specifier.clone(),
            imported: if binding.namespace {
                "*".into()
            } else {
                binding.imported.clone().unwrap_or_default()
            },
        }],
        None => vec![],
    }
}

pub(super) fn origin_targets(
    program: &ProgramIR,
    modules: &BTreeMap<PathBuf, &ModuleIR>,
    table: &ExportTable,
    module: &Path,
    namespace: SymbolNamespace,
    origin: &Origin,
    type_views: bool,
) -> Vec<ModuleBindingTarget> {
    match origin {
        Origin::Direct(target) => vec![target.clone()],
        Origin::Local(symbol) => modules
            .get(module)
            .and_then(|module| {
                module
                    .semantic
                    .symbols
                    .get(symbol.0)
                    .map(|symbol| symbol_targets(program, table, module, symbol, type_views))
            })
            .unwrap_or_default(),
        Origin::Imported {
            target: ResolvedModule::File { path },
            name,
        } => exported_targets(table, path, name, namespace, type_views).unwrap_or_default(),
        Origin::Imported {
            target: ResolvedModule::External { specifier },
            name,
        } => vec![ModuleBindingTarget::External {
            specifier: specifier.clone(),
            imported: name.clone(),
        }],
        Origin::Missing => vec![],
    }
}

fn exported_targets(
    table: &ExportTable,
    path: &Path,
    name: &str,
    namespace: SymbolNamespace,
    type_views: bool,
) -> Option<Vec<ModuleBindingTarget>> {
    let key = (path.to_path_buf(), name.to_owned(), namespace);
    if let Some(targets) = table.get(&key).filter(|targets| !targets.is_empty()) {
        return Some(targets.clone());
    }
    // TypeScript 的 import/export type 也可引用 const/function 供 typeof 使用。
    // 这是类型位置的导入视图，不会额外创建可在运行期使用的值绑定。
    if type_views && namespace == SymbolNamespace::Type {
        return table
            .get(&(path.to_path_buf(), name.to_owned(), SymbolNamespace::Value))
            .cloned();
    }
    None
}

pub(super) fn extend_unique(
    destination: &mut Vec<ModuleBindingTarget>,
    targets: Vec<ModuleBindingTarget>,
) {
    for target in targets {
        if !destination.contains(&target) {
            destination.push(target)
        }
    }
}

pub(super) fn link_diagnostic(
    code: &str,
    path: &Path,
    span: &SourceSpan,
    message: String,
) -> Diagnostic {
    Diagnostic { code: code.into(), category: DiagnosticCategory::Module, severity: Severity::Error, path: path.into(), span: Some(span.clone()), message,
        suggestion: "检查导出名称、type-only 限制和星号转导；使用显式导出消除歧义。external 星号导出需由调用方解析".into(), related: vec![] }
}
