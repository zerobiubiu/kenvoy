//! 一次索引 KEngine 已解析的 ESM 目标，支持 SDK 转导与跨模块常量。
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use kengine::{ModuleBindingTarget, ProgramIR, SymbolNamespace};

use super::{
    bindings::{Expression, Resolver, Target},
    source::Source,
};

impl<'a> Resolver<'a> {
    pub fn new(program: &'a ProgramIR, sources: &'a [Source<'a>]) -> Self {
        let paths: BTreeMap<&Path, usize> = sources
            .iter()
            .enumerate()
            .map(|(index, source)| (source.module.path.as_path(), index))
            .collect();
        let target = |target: &ModuleBindingTarget| match target {
            ModuleBindingTarget::External {
                specifier,
                imported,
            } if specifier == "@kenvoy/client" => match imported.as_str() {
                "useI18n" => Target::Hook,
                "*" => Target::Sdk,
                _ => Target::Other,
            },
            ModuleBindingTarget::Symbol { module, symbol } => paths
                .get(module.as_path())
                .map_or(Target::Other, |module| Target::Symbol(*module, *symbol)),
            ModuleBindingTarget::Expression { module, expression } => {
                paths.get(module.as_path()).map_or(Target::Other, |module| {
                    Target::Expression(Expression {
                        module: *module,
                        id: *expression,
                    })
                })
            }
            ModuleBindingTarget::Namespace { module } => paths
                .get(module.as_path())
                .map_or(Target::Other, |module| Target::Module(*module)),
            _ => Target::Other,
        };
        let imports = program
            .module_bindings
            .imports
            .iter()
            .filter_map(|import| {
                let module = *paths.get(import.module.as_path())?;
                let [value] = import.targets.as_slice() else {
                    return None;
                };
                Some(((module, import.symbol), target(value)))
            })
            .collect();
        let exports = program
            .module_bindings
            .exports
            .iter()
            .filter_map(|export| {
                if export.namespace != SymbolNamespace::Value {
                    return None;
                }
                let module = *paths.get(export.module.as_path())?;
                let [value] = export.targets.as_slice() else {
                    return None;
                };
                Some(((module, export.name.as_str()), target(value)))
            })
            .collect();
        Self {
            sources,
            imports,
            exports,
            origins: BTreeMap::new(),
            pending_origins: BTreeSet::new(),
            pending_values: BTreeSet::new(),
        }
    }
}
