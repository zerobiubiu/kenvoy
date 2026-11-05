use kengine::{
    DeclarationKind, ReferenceKind, ResolvedModule, ScopeId, SourceSpan, SymbolIR, SymbolId,
    SymbolKind, SymbolNamespace,
};

use crate::application::execution::JsDiagnostic;

use super::{ClientImportTarget, Collector, failure, raw};

impl Collector<'_> {
    pub(super) fn capture(&mut self, span: &SourceSpan) -> Result<(), JsDiagnostic> {
        for expression in &self.module.expressions {
            if within(&expression.span, span)
                && let Some(text) = self.module.raw(&expression.span)
            {
                let text = text.trim_start();
                if text.starts_with("import.meta")
                    || (expression.kind == "Call"
                        && text
                            .strip_prefix("import")
                            .and_then(|rest| rest.chars().next())
                            .is_some_and(|ch| ch == '(' || ch == '/' || ch.is_whitespace()))
                {
                    return Err(failure(self.reference, "客户端切片不允许动态模块加载"));
                }
            }
        }
        for reference in &self.module.semantic.references {
            if !within(&reference.span, span)
                || reference.namespace != SymbolNamespace::Value
                || matches!(
                    reference.kind,
                    ReferenceKind::Type | ReferenceKind::TypeQuery | ReferenceKind::Export
                )
            {
                continue;
            }
            let Some(symbol_id) = reference.symbol else {
                if matches!(reference.name.as_str(), "require" | "eval" | "Function") {
                    return Err(failure(
                        self.reference,
                        "客户端切片不允许未建模的代码或模块加载",
                    ));
                }
                continue;
            };
            let symbol = self
                .module
                .semantic
                .symbol(symbol_id)
                .ok_or_else(|| failure(self.reference, "词法引用失效"))?;
            if symbol
                .declarations
                .iter()
                .any(|declaration| within(declaration, span))
            {
                continue;
            }
            if symbol.scope != ScopeId(0) && !self.template_local(symbol)? {
                return Err(failure(
                    self.reference,
                    "客户端表达式捕获了声明模块之外的局部作用域",
                ));
            }
            self.pending.push_back(symbol_id);
        }
        Ok(())
    }

    pub(super) fn include(&mut self, id: SymbolId) -> Result<(), JsDiagnostic> {
        if !self.seen.insert(id) {
            return Ok(());
        }
        if self.seen.len() > 1024 {
            return Err(failure(self.reference, "客户端切片依赖超过 1024 个声明"));
        }
        let symbol = self
            .module
            .semantic
            .symbol(id)
            .ok_or_else(|| failure(self.reference, "找不到词法符号"))?;
        if let Some(import) = &symbol.import {
            let specifier = String::from_utf16(&import.source_utf16)
                .map_err(|_| failure(self.reference, "模块说明符不能转换为 UTF-8"))?;
            let declaration = self
                .module
                .imports
                .iter()
                .find(|declaration| {
                    declaration
                        .specifiers
                        .iter()
                        .any(|item| item.local.name == symbol.name && !item.type_only)
                })
                .ok_or_else(|| failure(self.reference, "客户端引用缺少普通 ESM 导入声明"))?;
            if declaration.attributes.is_some() || declaration.type_only {
                return Err(failure(
                    self.reference,
                    "客户端切片不支持导入属性或仅类型导入",
                ));
            }
            let target = self
                .program
                .module_graph
                .edges
                .iter()
                .find(|edge| {
                    edge.importer == self.module.path
                        && edge.specifier == specifier
                        && matches!(edge.kind, kengine::DependencyKind::Import)
                })
                .and_then(|edge| edge.target.as_ref())
                .ok_or_else(|| failure(self.reference, "无法解析客户端切片依赖"))?;
            let target = match target {
                ResolvedModule::File { path } => ClientImportTarget::File(path.clone()),
                ResolvedModule::External { specifier } => {
                    ClientImportTarget::External(specifier.clone())
                }
            };
            self.imports.insert(specifier.clone(), target);
            let quoted = serde_json::to_string(&specifier)
                .map_err(|_| failure(self.reference, "模块说明符序列化失败"))?;
            let clause = if import.namespace {
                format!("* as {}", symbol.name)
            } else if import.imported.as_deref() == Some("default") {
                symbol.name.clone()
            } else {
                let imported = serde_json::to_string(
                    import
                        .imported
                        .as_deref()
                        .ok_or_else(|| failure(self.reference, "导入名称缺失"))?,
                )
                .map_err(|_| failure(self.reference, "导入名称序列化失败"))?;
                format!("{{ {imported} as {} }}", symbol.name)
            };
            self.import_code
                .insert(import.span.start, format!("import {clause} from {quoted};"));
            return Ok(());
        }
        if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Const) {
            return Err(failure(
                self.reference,
                format!("客户端切片不支持捕获可变或复杂声明 {}", symbol.name),
            ));
        }
        let declaration = self
            .module
            .declarations
            .iter()
            .filter(|declaration| {
                symbol
                    .declarations
                    .iter()
                    .any(|span| within(span, &declaration.span))
            })
            .min_by_key(|declaration| declaration.span.end - declaration.span.start)
            .ok_or_else(|| failure(self.reference, "找不到客户端依赖的声明结构"))?;
        if declaration.kind == DeclarationKind::Function {
            let code = raw(self.module, &declaration.span, self.reference)?
                .trim_start_matches("export ")
                .trim_start_matches("default ")
                .to_owned();
            self.pieces.insert(declaration.span.start, code);
            self.capture(&declaration.span.clone())?;
        } else if declaration.kind == DeclarationKind::Const {
            let declarator = declaration
                .declarators
                .iter()
                .find(|item| {
                    symbol
                        .declarations
                        .iter()
                        .any(|span| within(span, &item.pattern.span))
                })
                .ok_or_else(|| failure(self.reference, "常量依赖缺少初始化结构"))?;
            if !matches!(
                declarator.pattern.kind,
                kengine::BindingPatternKind::Identifier(_)
            ) {
                return Err(failure(
                    self.reference,
                    "声明模块中的解构依赖请移到独立 UI 模块",
                ));
            }
            let initializer = declarator
                .initializer
                .and_then(|id| self.module.expression(id))
                .ok_or_else(|| failure(self.reference, "客户端常量必须具有初始值"))?;
            if !matches!(initializer.kind.as_str(), "Arrow" | "Function")
                && self.module.expressions.iter().any(|expr| {
                    within(&expr.span, &initializer.span)
                        && matches!(
                            expr.kind.as_str(),
                            "Call" | "New" | "Await" | "Yield" | "Assignment" | "Update"
                        )
                })
            {
                return Err(failure(
                    self.reference,
                    "声明模块的客户端依赖包含无法证明无副作用的初始化",
                ));
            }
            let code = format!(
                "const {};",
                raw(self.module, &declarator.span, self.reference)?
            );
            let pieces = if symbol.scope == ScopeId(0) {
                &mut self.pieces
            } else {
                &mut self.local_pieces
            };
            pieces.insert(declarator.span.start, code);
            self.capture(&declarator.span.clone())?;
        } else {
            return Err(failure(
                self.reference,
                "声明模块中的组件依赖请拆分为独立 UI 模块",
            ));
        }
        Ok(())
    }

    fn template_local(&mut self, symbol: &SymbolIR) -> Result<bool, JsDiagnostic> {
        if symbol.kind != SymbolKind::Const {
            return Ok(false);
        }
        let span = self
            .module
            .declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Function)
            .map(|declaration| &declaration.span)
            .chain(
                self.module
                    .expressions
                    .iter()
                    .filter(|expression| {
                        matches!(expression.kind.as_str(), "ArrowFunction" | "Function")
                    })
                    .map(|expression| &expression.span),
            )
            .filter(|span| {
                symbol
                    .declarations
                    .iter()
                    .all(|binding| within(binding, span))
            })
            .min_by_key(|span| span.end - span.start);
        let Some(span) = span else {
            return Ok(false);
        };
        let key = (span.start, span.end);
        if !self.template_bindings.contains_key(&key) {
            let template = self
                .template_parser
                .function_template(self.module, span)
                .map_err(|error| failure(self.reference, error.message))?;
            self.template_bindings.insert(
                key,
                template
                    .bindings
                    .into_iter()
                    .map(|(symbol, _)| symbol)
                    .collect(),
            );
        }
        Ok(self.template_bindings[&key].contains(&symbol.id))
    }
}

fn within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}
