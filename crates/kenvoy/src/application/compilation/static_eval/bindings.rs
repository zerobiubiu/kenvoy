use std::path::Path;

use kengine::{
    DependencyKind, ElementNameIR, ExpressionIR, ExpressionId, IdentifierIR, MemberPropertyIR,
    ModuleBindingTarget, ResolvedModule, SymbolNamespace,
};

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{Evaluator, ResolvedReference, evaluator::location};

impl Evaluator<'_> {
    pub(super) fn export_target(
        &self,
        module: &Path,
        name: &str,
        source: &SourceLocation,
    ) -> Result<ModuleBindingTarget, DslDiagnostic> {
        let targets = self
            .program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == module
                    && export.name == name
                    && export.namespace == SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .unwrap_or_default();
        match targets {
            [target] => Ok(target.clone()),
            _ => Err(self.error(
                "KV1104",
                source.clone(),
                format!("导出 {name} 缺失或存在歧义"),
                "使用唯一的值导出，检查 type-only 和星号转导",
            )),
        }
    }

    pub(super) fn identifier_target(
        &self,
        path: &Path,
        identifier: &IdentifierIR,
    ) -> Result<ModuleBindingTarget, DslDiagnostic> {
        let module = self.module(path)?;
        let source = location(module, identifier.span.clone());
        let symbol = module
            .semantic
            .references_at(identifier.span.start, identifier.span.end)
            .find(|reference| reference.namespace == SymbolNamespace::Value)
            .and_then(|reference| reference.symbol)
            .and_then(|id| module.semantic.symbol(id))
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    format!("无法确定 {} 的值绑定", identifier.name),
                    "使用已声明的 const 或明确导入的模块导出",
                )
            })?;
        if symbol.import.is_some() {
            let targets = self
                .program
                .module_bindings
                .imports
                .iter()
                .find(|import| import.module == path && import.symbol == symbol.id)
                .map(|import| import.targets.as_slice())
                .unwrap_or_default();
            return match targets {
                [target] => Ok(target.clone()),
                _ => Err(self.error(
                    "KV1104",
                    source,
                    "导入没有唯一的值目标",
                    "检查模块导出、type-only 声明和转导歧义",
                )),
            };
        }
        Ok(ModuleBindingTarget::Symbol {
            module: path.into(),
            symbol: symbol.id,
        })
    }

    pub(super) fn imported_reference(
        &self,
        path: &Path,
        identifier: &IdentifierIR,
        member: Option<&str>,
        source: &SourceLocation,
    ) -> Result<Option<ResolvedReference>, DslDiagnostic> {
        let module = self.module(path)?;
        let import = module
            .semantic
            .references_at(identifier.span.start, identifier.span.end)
            .find(|reference| reference.namespace == SymbolNamespace::Value)
            .and_then(|reference| reference.symbol)
            .and_then(|id| module.semantic.symbol(id))
            .and_then(|symbol| symbol.import.as_ref());
        let Some(import) = import else {
            return Ok(None);
        };
        if member.is_some() && !import.namespace {
            return Err(self.error(
                "KV1104",
                source.clone(),
                "嵌套模块成员不能作为运行时引用",
                "请直接导入可调用导出，或使用一层 import * as namespace 成员",
            ));
        }
        let specifier = String::from_utf16(&import.source_utf16).map_err(|_| {
            self.error(
                "KV1104",
                source.clone(),
                "导入模块说明符包含无效 Unicode",
                "修复模块说明符",
            )
        })?;
        let target = self
            .program
            .module_graph
            .edges
            .iter()
            .find(|edge| {
                edge.importer == path
                    && edge.specifier == specifier
                    && edge.kind == DependencyKind::Import
            })
            .and_then(|edge| edge.target.as_ref())
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "无法定位运行时导入路径",
                    "修复对应的值导入，不能仅使用 type-only 依赖",
                )
            })?;
        let export = if import.namespace {
            member.unwrap_or("*").to_owned()
        } else {
            import.imported.clone().ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "导入缺少有效导出名",
                    "使用明确且有效的导出名称",
                )
            })?
        };
        let (module, external) = match target {
            ResolvedModule::File { path } => (path.clone(), false),
            ResolvedModule::External { specifier } => (specifier.into(), true),
        };
        Ok(Some(ResolvedReference {
            module,
            export,
            external,
        }))
    }

    pub(super) fn namespace_member_target(
        &mut self,
        path: &Path,
        object: ExpressionId,
        property: &MemberPropertyIR,
        source: &SourceLocation,
    ) -> Result<Option<ModuleBindingTarget>, DslDiagnostic> {
        let expression = self.module(path)?.expression(object).cloned();
        let Some(ExpressionIR {
            direct_identifier: Some(identifier),
            ..
        }) = expression
        else {
            return Ok(None);
        };
        let target = self.identifier_target(path, &identifier)?;
        let name = match property {
            MemberPropertyIR::Identifier(name) => name.name.clone(),
            MemberPropertyIR::Computed { expression, .. } => {
                let value = self.evaluate_value(path, *expression)?;
                self.key_value(value, source)?
            }
            MemberPropertyIR::Private(_) => return Ok(None),
        };
        match target {
            ModuleBindingTarget::Namespace { module } => {
                self.export_target(&module, &name, source).map(Some)
            }
            ModuleBindingTarget::External {
                specifier,
                imported,
            } if imported == "*" => Ok(Some(ModuleBindingTarget::External {
                specifier,
                imported: name,
            })),
            _ => Ok(None),
        }
    }

    pub(super) fn element_reference(
        &mut self,
        path: &Path,
        name: &ElementNameIR,
        source: &SourceLocation,
    ) -> Result<Option<(ModuleBindingTarget, ResolvedReference)>, DslDiagnostic> {
        let identifier = match name {
            ElementNameIR::Identifier(identifier) => {
                let bound = self
                    .module(path)?
                    .semantic
                    .references_at(identifier.span.start, identifier.span.end)
                    .any(|reference| {
                        reference.namespace == SymbolNamespace::Value && reference.symbol.is_some()
                    });
                if !bound && is_intrinsic(&identifier.name) {
                    return Ok(None);
                }
                identifier.clone()
            }
            ElementNameIR::Member { parts, .. } => parts.first().cloned().ok_or_else(|| {
                self.error(
                    "KV1104",
                    source.clone(),
                    "组件成员名称为空",
                    "使用有效组件引用",
                )
            })?,
            _ => return Ok(None),
        };
        let mut target = self.identifier_target(path, &identifier)?;
        if let ElementNameIR::Member { parts, .. } = name {
            for part in parts.iter().skip(1) {
                target = match target {
                    ModuleBindingTarget::Namespace { module } => {
                        self.export_target(&module, &part.name, source)?
                    }
                    ModuleBindingTarget::External {
                        specifier,
                        imported,
                    } if imported == "*" => ModuleBindingTarget::External {
                        specifier,
                        imported: part.name.clone(),
                    },
                    _ => {
                        return Err(self.error(
                            "KV1104",
                            source.clone(),
                            "组件成员只支持模块 namespace 的导出",
                            "直接导入组件，不能通过运行时对象构造组件注册表",
                        ));
                    }
                };
            }
        }
        let leaf = self.reference_for_target(&target, source)?;
        // Schema 组合编译时展开，仍以最终声明模块判断；运行时引用保留导入门面，避免丢失其 CSS 和依赖边界。
        if !leaf.external && leaf.module.to_string_lossy().ends_with(".diu.tsx") {
            return Ok(Some((target, leaf)));
        }
        let member = if let ElementNameIR::Member { parts, .. } = name {
            if parts.len() != 2 {
                return Err(self.error(
                    "KV1104",
                    source.clone(),
                    "嵌套模块成员不能作为运行时组件引用",
                    "请直接导入组件，或使用一层 import * as namespace 成员",
                ));
            }
            Some(parts[1].name.as_str())
        } else {
            None
        };
        let reference = self
            .imported_reference(path, &identifier, member, source)?
            .unwrap_or(leaf);
        Ok(Some((target, reference)))
    }
}

fn is_intrinsic(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}
