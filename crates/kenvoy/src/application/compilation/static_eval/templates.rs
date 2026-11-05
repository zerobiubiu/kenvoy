use std::path::Path;

use kengine::{
    DeclarationKind, ExpressionIR, ExpressionId, ExpressionShape, ModuleBindingTarget, ModuleIR,
    ScopeId, SourceSpan, SymbolId, SymbolKind,
};

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{Evaluated, Evaluator, StaticNode, evaluator::location};

/// 只描述函数直接返回的模板和顺序声明的常量，不执行函数或解释控制流。
#[derive(Debug)]
pub struct FunctionTemplate {
    pub bindings: Vec<(SymbolId, ExpressionId)>,
    pub expression: ExpressionId,
}

/// 函数 statement 结构由 Kenvoy 的语法适配器提供，不扩展 Generic IR 的业务语义。
pub trait FunctionTemplateParser {
    fn function_template(
        &self,
        module: &ModuleIR,
        span: &SourceSpan,
    ) -> Result<FunctionTemplate, DslDiagnostic>;
}

pub(super) struct TemplateValue {
    value: Evaluated,
    nodes: usize,
    depth: usize,
}

impl TemplateValue {
    fn new(value: Evaluated) -> Self {
        let (nodes, depth) = match &value {
            Evaluated::Nodes(nodes) => node_stats(nodes),
            _ => (0, 0),
        };
        Self {
            value,
            nodes,
            depth,
        }
    }
}

impl Evaluator<'_> {
    pub(super) fn eval_entry_target(
        &mut self,
        target: ModuleBindingTarget,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        match target {
            ModuleBindingTarget::Expression { module, expression } => {
                self.ensure_static_module(&module)?;
                self.eval_entry_expr(&module, expression)
            }
            ModuleBindingTarget::Symbol {
                module: path,
                symbol,
            } => {
                let module = self.module(&path)?;
                let symbol = module.semantic.symbol(symbol).ok_or_else(|| {
                    self.error("KV1104", source.clone(), "符号引用无效", "重新分析源码")
                })?;
                if symbol.scope == ScopeId(0) {
                    self.ensure_static_module(&path)?;
                    if symbol.kind == SymbolKind::Const {
                        let initializer = self.const_initializer(module, symbol, source)?;
                        return self.eval_entry_expr(&path, initializer);
                    }
                    if symbol.kind == SymbolKind::Function {
                        let declaration = module
                            .declarations
                            .iter()
                            .find(|declaration| {
                                declaration.kind == DeclarationKind::Function
                                    && declaration
                                        .bindings
                                        .iter()
                                        .any(|binding| symbol.declarations.contains(&binding.span))
                            })
                            .ok_or_else(|| {
                                self.error(
                                    "KV1104",
                                    source.clone(),
                                    "函数缺少可定位的声明结构",
                                    "重新分析源码",
                                )
                            })?;
                        return self.eval_function_template(&path, &declaration.span);
                    }
                }
                self.eval_target(
                    ModuleBindingTarget::Symbol {
                        module: path,
                        symbol: symbol.id,
                    },
                    source,
                )
            }
            ModuleBindingTarget::Declaration { module, span } => {
                self.ensure_static_module(&module)?;
                self.eval_function_template(&module, &span)
            }
            target => self.eval_target(target, source),
        }
    }

    pub(super) fn eval_entry_shape(
        &mut self,
        path: &Path,
        expression: &ExpressionIR,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        if matches!(expression.kind.as_str(), "ArrowFunction" | "Function") {
            return self.eval_function_template(path, &expression.span);
        }
        match &expression.shape {
            ExpressionShape::Wrapper { expression, .. } => self.eval_entry_expr(path, *expression),
            ExpressionShape::Identifier => {
                let identifier = expression.direct_identifier.as_ref().ok_or_else(|| {
                    self.error(
                        "KV1104",
                        source.clone(),
                        "标识符缺少通用结构",
                        "重新分析源码",
                    )
                })?;
                self.eval_entry_target(self.identifier_target(path, identifier)?, source)
            }
            ExpressionShape::Member { object, property } => {
                if let Some(target) =
                    self.namespace_member_target(path, *object, property, source)?
                {
                    return self.eval_entry_target(target, source);
                }
                self.eval_shape(path, expression, source)
            }
            _ => self.eval_shape(path, expression, source),
        }
    }

    fn eval_function_template(
        &mut self,
        path: &Path,
        span: &SourceSpan,
    ) -> Result<Evaluated, DslDiagnostic> {
        let module = self.module(path)?;
        let source = location(module, span.clone());
        self.tick(&source)?;
        let template = self.template_parser.function_template(module, span)?;
        self.check_collection(template.bindings.len(), &source)?;
        self.template_module = Some(&module.path);
        let result = (|| {
            for (symbol, initializer) in template.bindings {
                let value = self.eval_expr(path, initializer)?;
                self.template_values
                    .insert(symbol, TemplateValue::new(value));
            }
            self.eval_expr(path, template.expression)
        })();
        self.template_values.clear();
        self.template_module = None;
        result
    }

    pub(super) fn template_value(
        &mut self,
        path: &Path,
        symbol: SymbolId,
        source: &SourceLocation,
    ) -> Result<Option<Evaluated>, DslDiagnostic> {
        if self.template_module != Some(path) {
            return Ok(None);
        }
        let Some(value) = self.template_values.get(&symbol) else {
            return Ok(None);
        };
        if self.node_depth + value.depth > self.limits.depth {
            return Err(self.error(
                "KV1103",
                source.clone(),
                "JSX 嵌套超过深度限制",
                "减少嵌套层级和模板常量组合",
            ));
        }
        self.nodes += value.nodes;
        if self.nodes > self.limits.nodes {
            return Err(self.error(
                "KV1103",
                source.clone(),
                "声明展开节点超过限制",
                "减少嵌套与重复的模板常量",
            ));
        }
        Ok(Some(value.value.clone()))
    }
}

fn node_stats(nodes: &[StaticNode]) -> (usize, usize) {
    nodes.iter().fold((0, 0), |(count, depth), node| {
        let (children, child_depth) = match node {
            StaticNode::Element(element) => node_stats(&element.children),
            StaticNode::Text { .. } => (0, 0),
        };
        (count + 1 + children, depth.max(1 + child_depth))
    })
}
