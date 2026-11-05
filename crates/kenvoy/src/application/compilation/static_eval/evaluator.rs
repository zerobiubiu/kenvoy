use std::{collections::BTreeSet, path::Path};

use kengine::{ExpressionId, ModuleIR, Position, ProgramIR, SourceSpan};
use serde_json::Value;

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{EvalLimits, Evaluated, Evaluator, FunctionTemplateParser, StaticNode};

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a ProgramIR, template_parser: &'a dyn FunctionTemplateParser) -> Self {
        Self::with_limits(program, template_parser, EvalLimits::default())
    }

    pub fn with_limits(
        program: &'a ProgramIR,
        template_parser: &'a dyn FunctionTemplateParser,
        limits: EvalLimits,
    ) -> Self {
        Self {
            program,
            template_parser,
            limits,
            steps: 0,
            nodes: 0,
            node_depth: 0,
            expressions: vec![],
            schemas: vec![],
            checked_modules: BTreeSet::new(),
            template_module: None,
            template_values: Default::default(),
        }
    }

    pub fn entry(&mut self, entry: &Path) -> Result<Vec<StaticNode>, DslDiagnostic> {
        let module = self.module(entry)?;
        let location = location(module, module.source_span());
        if self.program.has_errors() {
            return Err(self.error(
                "KV1104",
                location,
                "源项目存在解析、模块或绑定错误",
                "先修复 KEngine 诊断，再进行声明分析",
            ));
        }
        self.ensure_static_module(entry)?;
        let target = self.export_target(entry, "default", &location)?;
        match self.eval_entry_target(target, &location)? {
            Evaluated::Nodes(nodes) => Ok(nodes),
            _ => Err(self.error(
                "KV1101",
                location,
                "默认导出必须是静态 JSX 声明树或直接返回声明树的函数模板",
                "导出 JSX、Fragment、静态 const，或无参数且仅含静态 const 与直接 return 的函数",
            )),
        }
    }

    pub fn evaluate_value(
        &mut self,
        module: &Path,
        expression: ExpressionId,
    ) -> Result<Value, DslDiagnostic> {
        let source = self.expression_location(module, expression)?;
        match self.eval_expr(module, expression)? {
            Evaluated::Json(value) => Ok(value),
            _ => Err(self.error(
                "KV1101",
                source,
                "此处需要静态 JSON 值",
                "使用字面量或可确定的 const，不能用函数、组件或动态表达式",
            )),
        }
    }

    pub(super) fn module(&self, path: &Path) -> Result<&'a ModuleIR, DslDiagnostic> {
        self.program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    SourceLocation {
                        path: path.into(),
                        span: empty_span(),
                    },
                    "声明引用的模块不在分析结果中",
                    "使用项目内可解析模块，检查入口和模块边界",
                )
            })
    }

    pub(super) fn expression_location(
        &self,
        path: &Path,
        id: ExpressionId,
    ) -> Result<SourceLocation, DslDiagnostic> {
        let module = self.module(path)?;
        module
            .expression(id)
            .map(|expression| location(module, expression.span.clone()))
            .ok_or_else(|| {
                self.error(
                    "KV1104",
                    location(module, module.source_span()),
                    "表达式引用无效",
                    "重新分析项目，不能混用不同版本的 Generic IR 标识",
                )
            })
    }

    pub(super) fn error(
        &self,
        code: &str,
        source: SourceLocation,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> DslDiagnostic {
        let mut diagnostic = DslDiagnostic::new(code, source, message, suggestion);
        diagnostic.related = self
            .schemas
            .iter()
            .map(|(_, _, source)| source.clone())
            .collect();
        diagnostic
    }

    pub(super) fn tick(&mut self, source: &SourceLocation) -> Result<(), DslDiagnostic> {
        self.steps += 1;
        if self.steps > self.limits.steps
            || self.expressions.len() + self.schemas.len() >= self.limits.depth
        {
            return Err(self.error(
                "KV1103",
                source.clone(),
                "静态分析超过深度或运算预算",
                "拆分过深声明，减少递归组合和巨大常量",
            ));
        }
        Ok(())
    }

    pub(super) fn check_collection(
        &self,
        size: usize,
        source: &SourceLocation,
    ) -> Result<(), DslDiagnostic> {
        if size > self.limits.collection {
            Err(self.error(
                "KV1103",
                source.clone(),
                "静态集合超过大小限制",
                "减少数组、对象或展开属性的数量",
            ))
        } else {
            Ok(())
        }
    }
}

pub(super) fn empty_span() -> SourceSpan {
    SourceSpan {
        start: 0,
        end: 0,
        start_position: Position { line: 1, column: 1 },
        end_position: Position { line: 1, column: 1 },
    }
}
pub(super) fn location(module: &ModuleIR, span: SourceSpan) -> SourceLocation {
    SourceLocation {
        path: module.path.clone(),
        span,
    }
}

pub(super) trait ModuleSpan {
    fn source_span(&self) -> SourceSpan;
}
impl ModuleSpan for ModuleIR {
    fn source_span(&self) -> SourceSpan {
        self.semantic
            .scopes
            .first()
            .map(|scope| scope.span.clone())
            .unwrap_or_else(empty_span)
    }
}
