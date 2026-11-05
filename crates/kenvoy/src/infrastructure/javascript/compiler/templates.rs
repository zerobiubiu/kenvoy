//! 提取默认函数模板的直接返回值，不执行函数，也不把内部 JSX 出现位置当作返回值。

use kengine::{ExpressionId, ModuleIR, SourceSpan, SymbolKind};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

use crate::{
    application::compilation::static_eval::FunctionTemplate,
    domain::source::{DslDiagnostic, SourceLocation},
};

use super::parser;

pub(super) fn extract(
    module: &ModuleIR,
    span: &SourceSpan,
) -> Result<FunctionTemplate, DslDiagnostic> {
    let name = module.path.to_string_lossy();
    let extension = module.path.extension().and_then(|value| value.to_str());
    let typescript = matches!(extension, Some("ts" | "tsx" | "mts" | "cts"));
    let jsx = matches!(extension, Some("tsx" | "jsx"));
    let (_, parsed, base) =
        parser::parse(&module.source, &name, typescript, jsx).map_err(|error| {
            DslDiagnostic::new(
                "KV1104",
                error
                    .source
                    .map(|source| *source)
                    .unwrap_or_else(|| SourceLocation {
                        path: module.path.clone(),
                        span: span.clone(),
                    }),
                error.message,
                "先修复函数模板的语法诊断，再进行声明分析",
            )
        })?;
    let prefix = if module.source.starts_with('\u{feff}') {
        3
    } else {
        0
    };
    let mut finder = Finder {
        module,
        span,
        base,
        prefix,
        result: None,
    };
    parsed.visit_with(&mut finder);
    finder.result.unwrap_or_else(|| {
        Err(DslDiagnostic::new(
            "KV1101",
            SourceLocation {
                path: module.path.clone(),
                span: span.clone(),
            },
            "默认导出不是可静态提取的函数模板",
            "使用无参数的普通函数或箭头函数，直接返回 app/page 声明",
        ))
    })
}

struct Finder<'a> {
    module: &'a ModuleIR,
    span: &'a SourceSpan,
    base: u32,
    prefix: usize,
    result: Option<Result<FunctionTemplate, DslDiagnostic>>,
}

impl Finder<'_> {
    fn offsets(&self, span: Span) -> (usize, usize) {
        (
            span.lo.0.saturating_sub(self.base) as usize + self.prefix,
            span.hi.0.saturating_sub(self.base) as usize + self.prefix,
        )
    }

    fn matches(&self, span: Span) -> bool {
        self.offsets(span) == (self.span.start, self.span.end)
    }

    fn failure(&self, span: Span, message: &str) -> DslDiagnostic {
        let (start, end) = self.offsets(span);
        DslDiagnostic::new(
            "KV1101",
            parser::location(
                &self.module.path.to_string_lossy(),
                &self.module.source,
                start,
                end,
            ),
            message,
            "模板只允许静态 const 和一个直接 return；将 Hooks、控制流及运行时计算移到普通 UI 模块",
        )
    }

    fn expression(&self, expression: &ast::Expr) -> Result<ExpressionId, DslDiagnostic> {
        let (start, end) = self.offsets(expression.span());
        self.module
            .expressions
            .iter()
            .find(|value| value.span.start == start && value.span.end == end)
            .map(|value| value.id)
            .ok_or_else(|| self.failure(expression.span(), "模板表达式没有对应的 Generic IR 结构"))
    }

    fn function(&self, function: &ast::Function) -> Result<FunctionTemplate, DslDiagnostic> {
        if function.is_async || function.is_generator || !function.params.is_empty() {
            return Err(self.failure(
                function.span,
                "默认函数模板不能接收参数，也不能是 async 或 generator",
            ));
        }
        let body = function
            .body
            .as_ref()
            .ok_or_else(|| self.failure(function.span, "默认函数模板必须具有可静态读取的函数体"))?;
        self.block(body)
    }

    fn arrow(&self, arrow: &ast::ArrowExpr) -> Result<FunctionTemplate, DslDiagnostic> {
        if arrow.is_async || arrow.is_generator || !arrow.params.is_empty() {
            return Err(self.failure(
                arrow.span,
                "默认箭头模板不能接收参数，也不能是 async 或 generator",
            ));
        }
        match arrow.body.as_ref() {
            ast::ArrowFunctionBody::FunctionBody(body) => self.block(body),
            ast::ArrowFunctionBody::Expr(expression) => Ok(FunctionTemplate {
                bindings: vec![],
                expression: self.expression(expression)?,
            }),
        }
    }

    fn block(&self, body: &ast::FunctionBody) -> Result<FunctionTemplate, DslDiagnostic> {
        let mut bindings = Vec::new();
        let mut returned = None;
        for statement in &body.stmts {
            if matches!(statement, ast::Stmt::Empty(_)) {
                continue;
            }
            if returned.is_some() {
                return Err(self.failure(statement.span(), "模板 return 后不能再声明或执行语句"));
            }
            match statement {
                ast::Stmt::Decl(ast::Decl::Var(declaration))
                    if declaration.kind == ast::VarDeclKind::Const && !declaration.declare =>
                {
                    for declarator in &declaration.decls {
                        let ast::Pat::Ident(identifier) = &declarator.name else {
                            return Err(self.failure(
                                declarator.name.span(),
                                "模板内 const 必须使用独立标识符，不能使用解构或 Hooks 返回值",
                            ));
                        };
                        let initializer = declarator.init.as_ref().ok_or_else(|| {
                            self.failure(declarator.span, "模板内 const 必须具有静态初始值")
                        })?;
                        let (start, end) = self.offsets(identifier.id.span);
                        let symbol = self
                            .module
                            .semantic
                            .symbols
                            .iter()
                            .find(|symbol| {
                                symbol.kind == SymbolKind::Const
                                    && symbol
                                        .declarations
                                        .iter()
                                        .any(|span| span.start == start && span.end == end)
                            })
                            .ok_or_else(|| {
                                self.failure(identifier.id.span, "模板 const 缺少可定位的词法绑定")
                            })?;
                        bindings.push((symbol.id, self.expression(initializer)?));
                    }
                }
                ast::Stmt::Return(statement) => {
                    let expression = statement.arg.as_ref().ok_or_else(|| {
                        self.failure(statement.span, "函数模板必须直接返回静态 JSX 声明")
                    })?;
                    returned = Some(self.expression(expression)?);
                }
                _ => {
                    return Err(self.failure(
                        statement.span(),
                        "函数模板包含控制流、嵌套函数或运行时语句，不能静态提取 Schema",
                    ));
                }
            }
        }
        Ok(FunctionTemplate {
            bindings,
            expression: returned.ok_or_else(|| {
                self.failure(body.span, "函数模板缺少直接 return，不能从嵌套函数扫描 JSX")
            })?,
        })
    }
}

impl Visit for Finder<'_> {
    fn visit_fn_decl(&mut self, node: &ast::FnDecl) {
        if self.result.is_some() {
            return;
        }
        if self.matches(node.span()) {
            self.result = Some(self.function(&node.function));
        } else {
            node.visit_children_with(self);
        }
    }

    fn visit_fn_expr(&mut self, node: &ast::FnExpr) {
        if self.result.is_some() {
            return;
        }
        if self.matches(node.span()) {
            self.result = Some(self.function(&node.function));
        } else {
            node.visit_children_with(self);
        }
    }

    fn visit_arrow_expr(&mut self, node: &ast::ArrowExpr) {
        if self.result.is_some() {
            return;
        }
        if self.matches(node.span) {
            self.result = Some(self.arrow(node));
        } else {
            node.visit_children_with(self);
        }
    }
}
