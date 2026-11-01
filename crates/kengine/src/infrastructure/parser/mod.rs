mod expressions;
mod jsx;
mod modules;
mod patterns;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use swc_common::{FileName, SourceFile, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast as ast;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use swc_ecma_visit::{Visit, VisitWith};

use crate::{
    application::options::{ParseOptions, language},
    domain::{
        diagnostics::{Diagnostic, DiagnosticCategory, Severity},
        ir::{
            DeclarationIR, DeclarationKind, ExpressionId, GENERIC_IR_VERSION, IdentifierIR,
            ModuleIR, ModuleNameIR, NodeId, ParseOutput, StringLiteralIR,
        },
        language::Language,
        semantic::SemanticIR,
        source::{SourceIndex, SourceSpan},
    },
};

pub(crate) fn parse(path: &Path, source: &str, options: ParseOptions) -> ParseOutput {
    let language = match language(path, options) {
        Some(language) => language,
        None => {
            return ParseOutput {
                module: None,
                diagnostics: vec![Diagnostic {
                    code: "KE0001".into(),
                    category: DiagnosticCategory::Configuration,
                    severity: Severity::Error,
                    path: path.into(),
                    span: None,
                    message: "无法从文件名确定源语言".into(),
                    suggestion: "使用 .js/.jsx/.ts/.tsx 等扩展名，或显式指定 ParseOptions.language"
                        .into(),
                    related: vec![],
                }],
            };
        }
    };
    let syntax = match language {
        Language::TypeScript | Language::Tsx => Syntax::Typescript(TsSyntax {
            tsx: language == Language::Tsx,
            ..Default::default()
        }),
        _ => Syntax::Es(EsSyntax {
            jsx: language == Language::Jsx,
            ..Default::default()
        }),
    };
    let source_map: Lrc<SourceMap> = Default::default();
    let file = source_map.new_source_file(FileName::Real(path.into()).into(), source.to_owned());
    let mut recovered = Vec::new();
    let parsed = parse_file_as_module(&file, syntax, ast::EsVersion::EsNext, None, &mut recovered);
    let index = SourceIndex::new(source);
    // SourceMap 移除文件头 BOM；公开偏移仍指向用户的完整原始文件。
    let prefix = source.len().saturating_sub(file.src.len());
    let offset = |span: Span| {
        let start = span.lo.0.saturating_sub(file.start_pos.0) as usize + prefix;
        let end = span.hi.0.saturating_sub(file.start_pos.0) as usize + prefix;
        index.span(start.min(source.len()), end.min(source.len()))
    };
    let mut diagnostics: Vec<_> = recovered
        .iter()
        .map(|error| Diagnostic {
            code: "KE1001".into(),
            category: DiagnosticCategory::Syntax,
            severity: Severity::Error,
            path: path.into(),
            span: Some(offset(error.span())),
            message: format!("源代码语法错误：{}", error.kind().msg()),
            suggestion: "检查标注位置及相邻语法；成功恢复 AST 不表示源码有效".into(),
            related: vec![],
        })
        .collect();
    let module = match parsed {
        Ok(module) => {
            let mut converter = Converter::new(path, source, &file, prefix);
            converter.module_declarations(&module);
            module.visit_with(&mut converter);
            converter.output.semantic = super::semantic::analyze(&module, source, &file, prefix);
            Some(converter.output)
        }
        Err(error) => {
            diagnostics.push(Diagnostic {
                code: "KE1001".into(),
                category: DiagnosticCategory::Syntax,
                severity: Severity::Error,
                path: path.into(),
                span: Some(offset(error.span())),
                message: format!("源代码语法错误：{}", error.kind().msg()),
                suggestion: "修复标注位置的语法后重新解析".into(),
                related: vec![],
            });
            None
        }
    };
    ParseOutput {
        module,
        diagnostics,
    }
}

struct Converter<'a> {
    output: ModuleIR,
    file: &'a SourceFile,
    index: SourceIndex<'a>,
    prefix: usize,
    nodes: BTreeMap<(u32, u32, &'static str), NodeId>,
    expressions: BTreeMap<(u32, u32), ExpressionId>,
    exported: BTreeSet<(u32, u32)>,
    jsx_depth: usize,
}

impl<'a> Converter<'a> {
    fn new(path: &Path, source: &'a str, file: &'a SourceFile, prefix: usize) -> Self {
        Self {
            output: ModuleIR {
                format_version: GENERIC_IR_VERSION,
                path: path.into(),
                source: source.into(),
                imports: vec![],
                exports: vec![],
                declarations: vec![],
                nodes: vec![],
                expressions: vec![],
                jsx_roots: vec![],
                semantic: SemanticIR::default(),
            },
            file,
            index: SourceIndex::new(source),
            prefix,
            nodes: BTreeMap::new(),
            expressions: BTreeMap::new(),
            exported: BTreeSet::new(),
            jsx_depth: 0,
        }
    }

    fn span(&self, span: Span) -> SourceSpan {
        self.index.span(
            span.lo.0.saturating_sub(self.file.start_pos.0) as usize + self.prefix,
            span.hi.0.saturating_sub(self.file.start_pos.0) as usize + self.prefix,
        )
    }

    fn identifier(&self, name: &str, span: Span) -> IdentifierIR {
        IdentifierIR {
            name: name.into(),
            span: self.span(span),
        }
    }

    fn string(&self, value: &ast::Str) -> StringLiteralIR {
        StringLiteralIR {
            span: self.span(value.span),
            value_utf16: value.value.to_ill_formed_utf16().collect(),
        }
    }

    fn module_name(&self, name: &ast::ModuleExportName) -> ModuleNameIR {
        match name {
            ast::ModuleExportName::Ident(name) => {
                ModuleNameIR::Identifier(self.identifier(name.sym.as_ref(), name.span))
            }
            ast::ModuleExportName::Str(value) => ModuleNameIR::String(self.string(value)),
        }
    }
}

impl Visit for Converter<'_> {
    fn visit_expr(&mut self, expression: &ast::Expr) {
        self.expression(expression);
        expression.visit_children_with(self);
    }

    fn visit_jsx_element(&mut self, element: &ast::JSXElement) {
        let id = self.element(element);
        if self.jsx_depth == 0 {
            self.output.jsx_roots.push(id);
        }
        self.jsx_depth += 1;
        element.visit_children_with(self);
        self.jsx_depth -= 1;
    }

    fn visit_jsx_fragment(&mut self, fragment: &ast::JSXFragment) {
        let id = self.fragment(fragment);
        if self.jsx_depth == 0 {
            self.output.jsx_roots.push(id);
        }
        self.jsx_depth += 1;
        fragment.visit_children_with(self);
        self.jsx_depth -= 1;
    }

    fn visit_decl(&mut self, declaration: &ast::Decl) {
        let mut bindings = vec![];
        let mut declarators = vec![];
        let kind = match declaration {
            ast::Decl::Fn(value) => {
                bindings.push(self.identifier(value.ident.sym.as_ref(), value.ident.span));
                DeclarationKind::Function
            }
            ast::Decl::Class(value) => {
                bindings.push(self.identifier(value.ident.sym.as_ref(), value.ident.span));
                DeclarationKind::Class
            }
            ast::Decl::Var(value) => {
                for variable in &value.decls {
                    self.pattern(&variable.name, &mut bindings);
                    declarators.push(self.variable_declarator(variable));
                }
                match value.kind {
                    ast::VarDeclKind::Const => DeclarationKind::Const,
                    ast::VarDeclKind::Let => DeclarationKind::Let,
                    ast::VarDeclKind::Var => DeclarationKind::Var,
                }
            }
            ast::Decl::Using(value) => {
                for variable in &value.decls {
                    self.pattern(&variable.name, &mut bindings);
                    declarators.push(self.variable_declarator(variable));
                }
                DeclarationKind::Using
            }
            ast::Decl::TsInterface(value) => {
                bindings.push(self.identifier(value.id.sym.as_ref(), value.id.span));
                DeclarationKind::Interface
            }
            ast::Decl::TsTypeAlias(value) => {
                bindings.push(self.identifier(value.id.sym.as_ref(), value.id.span));
                DeclarationKind::TypeAlias
            }
            ast::Decl::TsEnum(value) => {
                bindings.push(self.identifier(value.id.sym.as_ref(), value.id.span));
                DeclarationKind::Enum
            }
            ast::Decl::TsModule(value) => {
                if let ast::TsModuleName::Ident(name) = &value.id {
                    bindings.push(self.identifier(name.sym.as_ref(), name.span));
                }
                DeclarationKind::Namespace
            }
        };
        let span = declaration.span();
        self.output.declarations.push(DeclarationIR {
            span: self.span(span),
            kind,
            bindings,
            exported: self.exported.contains(&(span.lo.0, span.hi.0)),
            declarators,
        });
        declaration.visit_children_with(self);
    }
}
