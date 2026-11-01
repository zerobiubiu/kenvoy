//! 从已分析的模块图准备不可变服务端白名单。

use kengine::{ProgramIR, ResolvedModule};
use std::collections::{BTreeMap, VecDeque};

use super::parser::{imports, restricted_parse, transpile};
use super::validation::validate;
use super::{MAX_BUNDLE_BYTES, MAX_FUNCTIONS, MAX_MODULES, failure, valid_specifier};
use crate::{
    application::execution::{
        BUNDLE_VERSION, FunctionTarget, JsDiagnostic, PreparedBundle, PreparedModule, Trust,
    },
    domain::model::{ApplicationIR, ModuleReferenceKind},
};
/// 只消费 KEngine 公共 IR；可达模块集合与路径均来自已经解析的模块图。
pub fn prepare(
    program: &ProgramIR,
    application: &ApplicationIR,
    trust: Trust,
) -> Result<PreparedBundle, JsDiagnostic> {
    if trust != Trust::Project {
        return Err(failure("尚未提供操作系统隔离，拒绝执行不可信 JavaScript"));
    }
    if program.has_errors() {
        return Err(failure("源码分析存在错误，不能准备服务端模块"));
    }
    let roots: Vec<_> = application
        .module_references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                ModuleReferenceKind::ServerFunction | ModuleReferenceKind::SharedFunction
            )
        })
        .collect();
    if roots.len() > MAX_FUNCTIONS {
        return Err(failure("服务端函数登记超过 1024 项限额"));
    }
    let mut queue: VecDeque<_> = roots
        .iter()
        .map(|reference| reference.module.clone())
        .collect();
    let mut compiled = BTreeMap::new();
    let mut total = 0usize;
    while let Some(path) = queue.pop_front() {
        if compiled.contains_key(&path) {
            continue;
        }
        if compiled.len() >= MAX_MODULES {
            return Err(failure("服务端可达模块超过 256 项限额"));
        }
        let module = program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(|| failure("服务端模块必须属于已分析的本地源码集合"))?;
        let name = path.to_string_lossy();
        restricted_parse(&module.source, &name, true)?;
        let code = transpile(&module.source, &name, false)?;
        total = total
            .checked_add(code.len())
            .ok_or_else(|| failure("模块总大小溢出"))?;
        if total > MAX_BUNDLE_BYTES {
            return Err(failure("服务端模块总大小超过 4 MiB 限额"));
        }
        let compiled_ast = restricted_parse(&code, &name, false)?;
        let mut dependencies = BTreeMap::new();
        for specifier in imports(&compiled_ast)? {
            if !valid_specifier(&specifier) {
                return Err(failure(format!(
                    "服务端不支持包、URL 或绝对路径依赖 {specifier}"
                )));
            }
            let target = program
                .module_graph
                .edges
                .iter()
                .find(|edge| edge.importer == path && edge.specifier == specifier)
                .and_then(|edge| edge.target.as_ref());
            let Some(ResolvedModule::File { path: dependency }) = target else {
                return Err(failure(format!(
                    "服务端静态依赖 {specifier} 没有已验证的本地模块目标"
                )));
            };
            dependencies.insert(specifier, dependency.clone());
            queue.push_back(dependency.clone());
        }
        compiled.insert(path, (code, dependencies));
    }
    let ids: BTreeMap<_, _> = compiled
        .keys()
        .enumerate()
        .map(|(index, path)| (path.clone(), format!("m_{}", index + 1)))
        .collect();
    let mut bundle = PreparedBundle {
        format_version: BUNDLE_VERSION,
        modules: BTreeMap::new(),
        functions: BTreeMap::new(),
    };
    for (path, (code, dependencies)) in compiled {
        let imports = dependencies
            .into_iter()
            .map(|(specifier, target)| {
                ids.get(&target)
                    .cloned()
                    .map(|id| (specifier, id))
                    .ok_or_else(|| failure("服务端依赖图闭包不完整"))
            })
            .collect::<Result<_, _>>()?;
        bundle
            .modules
            .insert(ids[&path].clone(), PreparedModule { code, imports });
    }
    for reference in roots {
        if reference.external || reference.expression.is_some() {
            return Err(failure("服务端函数必须使用本地静态导出登记"));
        }
        let module = ids
            .get(&reference.module)
            .cloned()
            .ok_or_else(|| failure("登记的服务端模块未编译"))?;
        if bundle
            .functions
            .insert(
                reference.id.clone(),
                FunctionTarget {
                    module,
                    export: reference.export.clone(),
                },
            )
            .is_some()
        {
            return Err(failure("服务端函数登记 ID 重复"));
        }
    }
    validate(&bundle)?;
    Ok(bundle)
}
