use std::path::Path;

use kengine::{
    BindingPatternKind, ExpressionShape, ModuleBindingTarget, NodeKind, ScopeId, ScopeKind,
    SymbolKind, SymbolNamespace,
};

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{Evaluated, Evaluator, ResolvedReference, evaluator::location};

impl Evaluator<'_> {
    pub(super) fn eval_target(
        &mut self,
        target: ModuleBindingTarget,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        match target {
            ModuleBindingTarget::Expression { module, expression } => {
                if self
                    .module(&module)?
                    .expression(expression)
                    .is_some_and(|value| {
                        matches!(value.kind.as_str(), "ArrowFunction" | "Function" | "Class")
                    })
                {
                    return self
                        .reference_for_target(
                            &ModuleBindingTarget::Expression { module, expression },
                            source,
                        )
                        .map(Evaluated::Reference);
                }
                self.ensure_static_module(&module)?;
                self.eval_expr(&module, expression)
            }
            ModuleBindingTarget::Symbol {
                module: path,
                symbol,
            } => {
                let module = self.module(&path)?;
                let symbol = module.semantic.symbol(symbol).ok_or_else(|| {
                    self.error("KV1104", source.clone(), "符号引用无效", "重新分析源码")
                })?;
                if symbol.kind == SymbolKind::Const {
                    if symbol.scope != ScopeId(0) {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "Schema 常量必须位于模块顶层",
                            "将静态数据声明为模块顶层 const",
                        ));
                    }
                    let declarator = module
                        .declarations
                        .iter()
                        .flat_map(|declaration| &declaration.declarators)
                        .find(|declarator| match &declarator.pattern.kind {
                            BindingPatternKind::Identifier(identifier) => symbol
                                .declarations
                                .iter()
                                .any(|span| span == &identifier.span),
                            _ => false,
                        });
                    if let Some(initializer) =
                        declarator.and_then(|declarator| declarator.initializer)
                    {
                        if module.expression(initializer).is_some_and(|value| {
                            matches!(value.kind.as_str(), "ArrowFunction" | "Function" | "Class")
                        }) {
                            return self
                                .reference_for_target(
                                    &ModuleBindingTarget::Symbol {
                                        module: path,
                                        symbol: symbol.id,
                                    },
                                    source,
                                )
                                .map(Evaluated::Reference);
                        }
                        self.ensure_static_module(&path)?;
                        return self.eval_expr(&path, initializer);
                    }
                    return Err(self.error(
                        "KV1101",
                        source.clone(),
                        "此 const 没有可直接定位的静态初始值",
                        "首版声明常量使用独立标识符绑定；解构结构保留但不进行值推导",
                    ));
                }
                if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Class) {
                    return self
                        .reference_for_target(
                            &ModuleBindingTarget::Symbol {
                                module: path,
                                symbol: symbol.id,
                            },
                            source,
                        )
                        .map(Evaluated::Reference);
                }
                Err(self.error(
                    "KV1101",
                    source.clone(),
                    "只有模块顶层 const 可以提供静态声明值",
                    "不要使用 let、var、参数、namespace 或运行时值推导 Schema",
                ))
            }
            target @ (ModuleBindingTarget::Declaration { .. }
            | ModuleBindingTarget::External { .. }) => self
                .reference_for_target(&target, source)
                .map(Evaluated::Reference),
            ModuleBindingTarget::Namespace { .. } => Err(self.error(
                "KV1101",
                source.clone(),
                "模块 namespace 本身不是 JSON 值",
                "选择明确的模块导出成员",
            )),
        }
    }

    pub(super) fn reference_for_target(
        &self,
        target: &ModuleBindingTarget,
        source: &SourceLocation,
    ) -> Result<ResolvedReference, DslDiagnostic> {
        if let ModuleBindingTarget::External {
            specifier,
            imported,
        } = target
        {
            return Ok(ResolvedReference {
                module: specifier.into(),
                export: imported.clone(),
                external: true,
            });
        }
        let path = match target {
            ModuleBindingTarget::Symbol { module, .. }
            | ModuleBindingTarget::Expression { module, .. }
            | ModuleBindingTarget::Declaration { module, .. }
            | ModuleBindingTarget::Namespace { module } => module,
            _ => unreachable!(),
        };
        let export = self.program.module_bindings.exports.iter().find(|export| {
            export.module == *path
                && export.namespace == SymbolNamespace::Value
                && export.targets.as_slice() == [target.clone()]
        });
        let Some(export) = export else {
            return Err(self.error(
                "KV1104",
                source.clone(),
                "运行时组件或函数必须具有可导入的导出",
                "给函数或组件添加 export，并使用明确的模块引用",
            ));
        };
        Ok(ResolvedReference {
            module: path.clone(),
            export: export.name.clone(),
            external: false,
        })
    }

    pub(super) fn ensure_static_module(&mut self, path: &Path) -> Result<(), DslDiagnostic> {
        if self.checked_modules.contains(path) {
            return Ok(());
        }
        let module = self.module(path)?;
        for expression in &module.expressions {
            if !matches!(
                expression.kind.as_str(),
                "Call" | "New" | "Assignment" | "Await" | "Yield"
            ) && !(expression.kind == "OtherExpression"
                && expression.shape == ExpressionShape::Opaque)
            {
                continue;
            }
            let inside_function = module.semantic.scopes.iter().any(|scope| {
                matches!(
                    scope.kind,
                    ScopeKind::Function | ScopeKind::FunctionParameters
                ) && scope.span.start <= expression.span.start
                    && scope.span.end >= expression.span.end
            });
            // JSX 内的表达式由对应位置处理：Schema 要求静态值，UI Props 保留客户端引用。
            let inside_jsx = module.nodes.iter().any(|node| {
                matches!(node.kind, NodeKind::Element(_) | NodeKind::Fragment { .. })
                    && node.span.start <= expression.span.start
                    && node.span.end >= expression.span.end
            });
            if !inside_function && !inside_jsx {
                return Err(self.error(
                    "KV1101",
                    location(module, expression.span.clone()),
                    "静态声明模块包含顶层调用、赋值或其他运行时副作用",
                    "把计算移入明确注册的函数或普通 UI 模块；Schema 模块只保留静态声明",
                ));
            }
        }
        self.checked_modules.insert(path.into());
        Ok(())
    }
}
