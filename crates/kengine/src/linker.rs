use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::*;

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

type ExportKey = (PathBuf, String, SymbolNamespace);
type ExportTable = BTreeMap<ExportKey, Vec<ModuleBindingTarget>>;

#[derive(Clone)]
enum Origin {
    Local(SymbolId),
    Imported {
        target: ResolvedModule,
        name: String,
    },
    Direct(ModuleBindingTarget),
    Missing,
}

struct Rule {
    key: ExportKey,
    origin: Origin,
    span: SourceSpan,
    type_only: bool,
}

pub(crate) fn link_project(program: &mut ProgramIR) {
    let modules: BTreeMap<_, _> = program
        .modules
        .iter()
        .map(|module| (module.path.clone(), module))
        .collect();
    let mut rules = Vec::new();
    let mut stars = Vec::new();
    let mut external_stars = Vec::new();
    let mut diagnostics = Vec::new();
    for module in &program.modules {
        for export in &module.exports {
            let target = source_target(program, &module.path, export.source.as_ref());
            match &export.kind {
                ExportKind::Declaration { declaration_span } => {
                    for symbol in &module.semantic.symbols {
                        if symbol.scope == ScopeId(0)
                            && symbol
                                .declarations
                                .iter()
                                .any(|span| within(span, declaration_span))
                        {
                            rules.push(Rule {
                                key: (module.path.clone(), symbol.name.clone(), symbol.namespace),
                                origin: Origin::Local(symbol.id),
                                span: export.span.clone(),
                                type_only: symbol.namespace == SymbolNamespace::Type,
                            });
                        }
                    }
                }
                ExportKind::DefaultExpression { expression } => {
                    rules.push(Rule {
                        key: (
                            module.path.clone(),
                            "default".into(),
                            SymbolNamespace::Value,
                        ),
                        origin: Origin::Direct(ModuleBindingTarget::Expression {
                            module: module.path.clone(),
                            expression: *expression,
                        }),
                        span: export.span.clone(),
                        type_only: false,
                    });
                }
                ExportKind::DefaultDeclaration {
                    declaration_span,
                    name,
                } => {
                    let mut found = false;
                    if let Some(name) = name {
                        for namespace in namespaces(false) {
                            if let Some(symbol) = local_symbol(module, &name.name, namespace) {
                                found = true;
                                rules.push(Rule {
                                    key: (module.path.clone(), "default".into(), namespace),
                                    origin: Origin::Local(symbol),
                                    span: export.span.clone(),
                                    type_only: namespace == SymbolNamespace::Type,
                                });
                            }
                        }
                    }
                    if !found {
                        rules.push(Rule {
                            key: (
                                module.path.clone(),
                                "default".into(),
                                SymbolNamespace::Value,
                            ),
                            origin: Origin::Direct(ModuleBindingTarget::Declaration {
                                module: module.path.clone(),
                                span: declaration_span.clone(),
                            }),
                            span: export.span.clone(),
                            type_only: false,
                        });
                    }
                }
                ExportKind::All => match target {
                    Some(ResolvedModule::File { path }) => {
                        stars.push((module.path.clone(), path, export.type_only))
                    }
                    Some(ResolvedModule::External { specifier }) => {
                        external_stars.push(ExternalStarExportIR {
                            module: module.path.clone(),
                            specifier,
                            type_only: export.type_only,
                            span: export.span.clone(),
                        })
                    }
                    None => {}
                },
                ExportKind::Named => {
                    for specifier in &export.specifiers {
                        let Some(exported) = module_name(&specifier.exported) else {
                            diagnostics.push(link_diagnostic(
                                "KE2103",
                                &module.path,
                                &specifier.span,
                                "导出名称含无法无损转换为标识名称的 UTF-16 code units".into(),
                            ));
                            continue;
                        };
                        let local = match &specifier.local {
                            Some(name) => match module_name(name) {
                                Some(name) => name,
                                None => {
                                    diagnostics.push(link_diagnostic("KE2103", &module.path, &specifier.span,
                                        "转导的来源名称含无法无损转换为标识名称的 UTF-16 code units".into()));
                                    continue;
                                }
                            },
                            None => "default".into(),
                        };
                        let type_only = export.type_only || specifier.type_only;
                        for namespace in namespaces(type_only) {
                            let origin = if specifier.kind == ExportSpecifierKind::Namespace {
                                match &target {
                                    Some(ResolvedModule::File { path }) => {
                                        Origin::Direct(ModuleBindingTarget::Namespace {
                                            module: path.clone(),
                                        })
                                    }
                                    Some(ResolvedModule::External { specifier }) => {
                                        Origin::Direct(ModuleBindingTarget::External {
                                            specifier: specifier.clone(),
                                            imported: "*".into(),
                                        })
                                    }
                                    None => Origin::Missing,
                                }
                            } else {
                                match &target {
                                    Some(target) => Origin::Imported {
                                        target: target.clone(),
                                        name: local.clone(),
                                    },
                                    None if export.source.is_none() => {
                                        local_symbol(module, &local, namespace)
                                            .or_else(|| {
                                                if type_only && namespace == SymbolNamespace::Type {
                                                    local_symbol(
                                                        module,
                                                        &local,
                                                        SymbolNamespace::Value,
                                                    )
                                                } else {
                                                    None
                                                }
                                            })
                                            .map(Origin::Local)
                                            .unwrap_or(Origin::Missing)
                                    }
                                    None => Origin::Missing,
                                }
                            };
                            rules.push(Rule {
                                key: (module.path.clone(), exported.clone(), namespace),
                                origin,
                                span: specifier.span.clone(),
                                type_only,
                            });
                        }
                    }
                }
                // TypeScript 的 export= 与全局 namespace 不是 ESM 命名导出，原结构仍在 ExportIR。
                ExportKind::TypeScriptAssignment { .. }
                | ExportKind::TypeScriptNamespace { .. } => {}
            }
        }
    }
    let explicit: BTreeSet<_> = rules.iter().map(|rule| rule.key.clone()).collect();
    let mut table = ExportTable::new();
    // 先求真实类型绑定闭包，再为 typeof 导入补充值的类型视图，避免提前 fallback
    // 把经过较长转导路径才到达的同名类型误判为另一个候选。
    for type_views in [false, true] {
        loop {
            let mut next = table.clone();
            for rule in &rules {
                let values = origin_targets(
                    program,
                    &modules,
                    &table,
                    &rule.key.0,
                    rule.key.2,
                    &rule.origin,
                    type_views,
                );
                extend_unique(next.entry(rule.key.clone()).or_default(), values);
            }
            for (module, source, type_only) in &stars {
                for ((path, name, namespace), targets) in &table {
                    if path != source || name == "default" {
                        continue;
                    }
                    let export_namespace = if *type_only {
                        if *namespace == SymbolNamespace::Value
                            && (!type_views
                                || table
                                    .get(&(path.clone(), name.clone(), SymbolNamespace::Type))
                                    .is_some_and(|targets| !targets.is_empty()))
                        {
                            continue;
                        }
                        SymbolNamespace::Type
                    } else {
                        *namespace
                    };
                    let key = (module.clone(), name.clone(), export_namespace);
                    if !explicit.contains(&key) {
                        extend_unique(next.entry(key).or_default(), targets.clone());
                    }
                }
            }
            if table == next {
                break;
            }
            table = next;
        }
    }
    let mut reported_exports = BTreeSet::new();
    for rule in &rules {
        let targets = table.get(&rule.key).cloned().unwrap_or_default();
        let other_space = if rule.key.2 == SymbolNamespace::Value {
            SymbolNamespace::Type
        } else {
            SymbolNamespace::Value
        };
        let other = table.get(&(rule.key.0.clone(), rule.key.1.clone(), other_space));
        // 普通 TS export 可导出值或类型；只有两个空间均空时报告缺失。
        if targets.len() > 1 && reported_exports.insert((rule.key.0.clone(), rule.span.start)) {
            diagnostics.push(link_diagnostic(
                "KE2102",
                &rule.key.0,
                &rule.span,
                format!("导出 {} 经转导指向多个不同绑定", rule.key.1),
            ));
        } else if targets.is_empty()
            && (rule.type_only || other.is_none_or(Vec::is_empty))
            && reported_exports.insert((rule.key.0.clone(), rule.span.start))
        {
            diagnostics.push(link_diagnostic(
                "KE2101",
                &rule.key.0,
                &rule.span,
                format!("导出 {} 没有可解析的绑定", rule.key.1),
            ));
        }
    }
    let mut imports = Vec::new();
    for module in &program.modules {
        let mut reported_imports = BTreeSet::new();
        for symbol in &module.semantic.symbols {
            let Some(binding) = &symbol.import else {
                continue;
            };
            let targets = symbol_targets(program, &table, module, symbol, true);
            if binding.imported.is_none()
                && !binding.namespace
                && reported_imports.insert(binding.span.start)
            {
                diagnostics.push(link_diagnostic(
                    "KE2103",
                    &module.path,
                    &binding.span,
                    "导入名称含无法无损转换为标识名称的 UTF-16 code units".into(),
                ));
            }
            let other_targets = module
                .semantic
                .symbols
                .iter()
                .filter(|other| {
                    other.name == symbol.name
                        && other.namespace != symbol.namespace
                        && other.import.is_some()
                })
                .flat_map(|other| symbol_targets(program, &table, module, other, true))
                .collect::<Vec<_>>();
            // 未解析文件边已产生定位诊断；external 没有安装/导出真实性保证。
            if targets.is_empty()
                && other_targets.is_empty()
                && reported_imports.insert(binding.span.start)
                && binding.imported.is_some()
            {
                diagnostics.push(link_diagnostic(
                    "KE2101",
                    &module.path,
                    &binding.span,
                    format!("导入 {} 没有可解析的导出绑定", symbol.name),
                ));
            } else if targets.len() > 1 && reported_imports.insert(binding.span.start) {
                diagnostics.push(link_diagnostic(
                    "KE2102",
                    &module.path,
                    &binding.span,
                    format!("导入 {} 经星号转导指向多个不同绑定", symbol.name),
                ));
            }
            imports.push(LinkedImportIR {
                module: module.path.clone(),
                symbol: symbol.id,
                targets,
            });
        }
    }
    program.module_bindings = ModuleBindingsIR {
        exports: table
            .into_iter()
            .filter(|(_, targets)| !targets.is_empty())
            .map(|((module, name, namespace), targets)| LinkedExportIR {
                module,
                name,
                namespace,
                targets,
            })
            .collect(),
        imports,
        external_star_exports: external_stars,
    };
    program.diagnostics.extend(diagnostics);
}

fn namespaces(type_only: bool) -> Vec<SymbolNamespace> {
    if type_only {
        vec![SymbolNamespace::Type]
    } else {
        vec![SymbolNamespace::Value, SymbolNamespace::Type]
    }
}

fn within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

fn module_name(name: &ModuleNameIR) -> Option<String> {
    match name {
        ModuleNameIR::Identifier(identifier) => Some(identifier.name.clone()),
        ModuleNameIR::String(value) => value.to_string().ok(),
    }
}

fn local_symbol(module: &ModuleIR, name: &str, namespace: SymbolNamespace) -> Option<SymbolId> {
    module
        .semantic
        .symbols
        .iter()
        .find(|symbol| {
            symbol.scope == ScopeId(0) && symbol.name == name && symbol.namespace == namespace
        })
        .map(|symbol| symbol.id)
}

fn source_target(
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

fn symbol_targets(
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

fn origin_targets(
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

fn extend_unique(destination: &mut Vec<ModuleBindingTarget>, targets: Vec<ModuleBindingTarget>) {
    for target in targets {
        if !destination.contains(&target) {
            destination.push(target)
        }
    }
}

fn link_diagnostic(code: &str, path: &Path, span: &SourceSpan, message: String) -> Diagnostic {
    Diagnostic { code: code.into(), category: DiagnosticCategory::Module, severity: Severity::Error, path: path.into(), span: Some(span.clone()), message,
        suggestion: "检查导出名称、type-only 限制和星号转导；使用显式导出消除歧义。external 星号导出需由调用方解析".into(), related: vec![] }
}
