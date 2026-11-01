//! 持久化模块包的版本、依赖与导出闭包校验。

use std::collections::{BTreeMap, BTreeSet};
use swc_ecma_ast as ast;

use super::parser::{imports, restricted_parse, text};
use super::{MAX_BUNDLE_BYTES, MAX_FUNCTIONS, MAX_MODULES, failure, valid_specifier};
use crate::application::execution::{BUNDLE_VERSION, JsDiagnostic, PreparedBundle};
fn valid_module_id(id: &str) -> bool {
    id.strip_prefix("m_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 10
            && !suffix.starts_with('0')
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn export_name(name: &ast::ModuleExportName) -> Result<String, JsDiagnostic> {
    match name {
        ast::ModuleExportName::Ident(name) => Ok(name.sym.to_string()),
        ast::ModuleExportName::Str(name) => text(name),
    }
}

fn pattern_names(pattern: &ast::Pat, names: &mut BTreeSet<String>) {
    match pattern {
        ast::Pat::Ident(name) => {
            names.insert(name.id.sym.to_string());
        }
        ast::Pat::Array(array) => {
            for item in array.elems.iter().flatten() {
                pattern_names(item, names);
            }
        }
        ast::Pat::Object(object) => {
            for item in &object.props {
                match item {
                    ast::ObjectPatProp::KeyValue(item) => pattern_names(&item.value, names),
                    ast::ObjectPatProp::Assign(item) => {
                        names.insert(item.key.id.sym.to_string());
                    }
                    ast::ObjectPatProp::Rest(item) => pattern_names(&item.arg, names),
                }
            }
        }
        ast::Pat::Assign(item) => pattern_names(&item.left, names),
        ast::Pat::Rest(item) => pattern_names(&item.arg, names),
        _ => {}
    }
}

/// 构建产物不是可信输入；拒绝未知版本、路径替代模块 ID、漏项映射和动态加载能力。
pub fn validate(bundle: &PreparedBundle) -> Result<(), JsDiagnostic> {
    if bundle.format_version != BUNDLE_VERSION
        || bundle.modules.len() > MAX_MODULES
        || bundle.functions.len() > MAX_FUNCTIONS
    {
        return Err(failure("JavaScript 产物版本或登记数量不受支持"));
    }
    let mut total = 0usize;
    let mut exports = BTreeMap::<String, BTreeSet<String>>::new();
    let mut stars = Vec::new();
    for (id, module) in &bundle.modules {
        if !valid_module_id(id) {
            return Err(failure("JavaScript 产物包含非法模块 ID"));
        }
        total = total
            .checked_add(module.code.len())
            .ok_or_else(|| failure("模块总大小溢出"))?;
        if total > MAX_BUNDLE_BYTES {
            return Err(failure("JavaScript 产物超过 4 MiB 限额"));
        }
        let ast = restricted_parse(&module.code, id, false)?;
        let required = imports(&ast)?;
        if required != module.imports.keys().cloned().collect() {
            return Err(failure("模块 import 白名单与代码中的静态依赖不一致"));
        }
        for (specifier, target) in &module.imports {
            if !valid_specifier(specifier) || !bundle.modules.contains_key(target) {
                return Err(failure("模块依赖不在已登记的本地白名单中"));
            }
        }
        let mut names = BTreeSet::new();
        for item in ast.body {
            let ast::ModuleItem::ModuleDecl(decl) = item else {
                continue;
            };
            match decl {
                ast::ModuleDecl::ExportDefaultDecl(_) | ast::ModuleDecl::ExportDefaultExpr(_) => {
                    names.insert("default".into());
                }
                ast::ModuleDecl::ExportDecl(export) => match export.decl {
                    ast::Decl::Fn(item) => {
                        names.insert(item.ident.sym.to_string());
                    }
                    ast::Decl::Class(item) => {
                        names.insert(item.ident.sym.to_string());
                    }
                    ast::Decl::Var(item) => {
                        for declaration in item.decls {
                            pattern_names(&declaration.name, &mut names);
                        }
                    }
                    _ => {}
                },
                ast::ModuleDecl::ExportNamed(export) => {
                    for specifier in export.specifiers {
                        names.insert(match specifier {
                            ast::ExportSpecifier::Named(item) => {
                                export_name(item.exported.as_ref().unwrap_or(&item.orig))?
                            }
                            ast::ExportSpecifier::Namespace(item) => export_name(&item.name)?,
                            ast::ExportSpecifier::Default(item) => item.exported.sym.to_string(),
                        });
                    }
                }
                ast::ModuleDecl::ExportAll(export) => {
                    stars.push((id.clone(), module.imports[&text(&export.src)?].clone()));
                }
                _ => {}
            }
        }
        exports.insert(id.clone(), names);
    }
    for _ in 0..bundle.modules.len() {
        let mut changed = false;
        for (id, target) in &stars {
            let target_names: Vec<_> = exports[target]
                .iter()
                .filter(|name| name.as_str() != "default")
                .cloned()
                .collect();
            let names = exports
                .get_mut(id)
                .ok_or_else(|| failure("模块导出闭包缺失"))?;
            for name in target_names {
                changed |= names.insert(name);
            }
        }
        if !changed {
            break;
        }
    }
    for (id, target) in &bundle.functions {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || target.export.is_empty()
            || target.export.len() > 256
            || target.export.contains('\0')
            || !exports
                .get(&target.module)
                .is_some_and(|names| names.contains(&target.export))
        {
            return Err(failure("JavaScript 函数登记 ID、模块或导出不存在"));
        }
    }
    Ok(())
}
