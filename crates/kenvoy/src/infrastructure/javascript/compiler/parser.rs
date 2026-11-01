//! 模块语法、能力限制与 TypeScript 转换。

use std::{collections::BTreeSet, path::PathBuf};
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::{self as ast, EsVersion};
use swc_ecma_codegen::to_code_default;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use swc_ecma_transforms_base::{fixer::fixer, resolver};
use swc_ecma_transforms_typescript::strip;
use swc_ecma_visit::{Visit, VisitWith};

use super::{MAX_MODULE_BYTES, failure};
use crate::{application::execution::JsDiagnostic, domain::source::SourceLocation};
fn location(name: &str, source: &str, start: usize, end: usize) -> SourceLocation {
    let position = |offset: usize| {
        let prefix = &source[..offset.min(source.len())];
        let mut line = 1;
        let mut column = 1;
        let mut last_cr = false;
        for ch in prefix.chars() {
            match ch {
                '\n' if last_cr => {
                    last_cr = false;
                }
                '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                    line += 1;
                    column = 1;
                    last_cr = ch == '\r';
                }
                _ => {
                    column += 1;
                    last_cr = false;
                }
            }
        }
        kengine::Position { line, column }
    };
    SourceLocation {
        path: PathBuf::from(name),
        span: kengine::SourceSpan {
            start,
            end,
            start_position: position(start),
            end_position: position(end),
        },
    }
}

fn parse(
    source: &str,
    name: &str,
    typescript: bool,
    jsx: bool,
) -> Result<(Lrc<SourceMap>, ast::Module, u32), JsDiagnostic> {
    if source.len() > MAX_MODULE_BYTES {
        return Err(failure("单个 JavaScript 模块超过 512 KiB 限额"));
    }
    let map: Lrc<SourceMap> = Default::default();
    let file = map.new_source_file(FileName::Custom(name.into()).into(), source.to_owned());
    let base = file.start_pos.0;
    let syntax = if typescript {
        Syntax::Typescript(TsSyntax {
            tsx: jsx,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax {
            jsx,
            ..Default::default()
        })
    };
    let mut recovered = Vec::new();
    let result = parse_file_as_module(&file, syntax, EsVersion::latest(), None, &mut recovered);
    let error = match result {
        Ok(module) if recovered.is_empty() => return Ok((map, module, base)),
        Ok(_) => recovered.remove(0),
        Err(error) => error,
    };
    let span = error.span();
    Err(JsDiagnostic::new(
        "KV6002",
        format!("JavaScript 模块语法无效：{:?}", error.kind()),
        Some(location(
            name,
            source,
            span.lo.0.saturating_sub(base) as usize,
            span.hi.0.saturating_sub(base) as usize,
        )),
        "修正模块语法后重新构建",
    ))
}

/// 客户端源码切片复用此转换器；JSX 保留给前端构建工具，不生成浏览器运行代码。
pub(super) fn transpile(source: &str, name: &str, jsx: bool) -> Result<String, JsDiagnostic> {
    let (map, module, _) = parse(source, name, true, jsx)?;
    Ok(GLOBALS.set(&Globals::default(), || {
        let unresolved = Mark::new();
        let top_level = Mark::new();
        let program = ast::Program::Module(module)
            .apply(resolver(unresolved, top_level, true))
            .apply(strip(unresolved, top_level))
            .apply(fixer(None));
        to_code_default(map, None, &program)
    }))
}

#[derive(Default)]
struct Restrictions {
    denied: Option<(Span, &'static str)>,
}
impl Restrictions {
    fn deny(&mut self, span: Span, reason: &'static str) {
        if self.denied.is_none() {
            self.denied = Some((span, reason));
        }
    }
}
impl Visit for Restrictions {
    fn visit_jsx_element(&mut self, node: &ast::JSXElement) {
        self.deny(node.span, "服务端计算模块不支持 JSX");
    }
    fn visit_jsx_fragment(&mut self, node: &ast::JSXFragment) {
        self.deny(node.span, "服务端计算模块不支持 JSX Fragment");
    }
    fn visit_call_expr(&mut self, node: &ast::CallExpr) {
        if matches!(node.callee, ast::Callee::Import(_)) {
            self.deny(node.span, "服务端模块不支持动态 import");
        }
        node.visit_children_with(self);
    }
    fn visit_meta_prop_expr(&mut self, node: &ast::MetaPropExpr) {
        if node.kind == ast::MetaPropKind::ImportMeta {
            self.deny(node.span, "服务端模块不开放 import.meta");
        }
    }
    fn visit_ident(&mut self, node: &ast::Ident) {
        if matches!(node.sym.as_ref(), "require" | "eval" | "Function") {
            self.deny(node.span, "服务端模块不支持 require 或动态代码构造");
        }
    }
    fn visit_import_decl(&mut self, node: &ast::ImportDecl) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 import attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_named_export(&mut self, node: &ast::NamedExport) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 export attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_export_all(&mut self, node: &ast::ExportAll) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 export attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_ts_import_equals_decl(&mut self, node: &ast::TsImportEqualsDecl) {
        self.deny(node.span, "服务端模块不支持 TypeScript import =");
    }
    fn visit_ts_export_assignment(&mut self, node: &ast::TsExportAssignment) {
        self.deny(node.span, "服务端模块不支持 TypeScript export =");
    }
}

pub(super) fn restricted_parse(
    source: &str,
    name: &str,
    typescript: bool,
) -> Result<ast::Module, JsDiagnostic> {
    let jsx = typescript && (name.ends_with(".tsx") || name.ends_with(".jsx"));
    let (_, module, base) = parse(source, name, typescript, jsx)?;
    let mut restrictions = Restrictions::default();
    module.visit_with(&mut restrictions);
    if let Some((span, reason)) = restrictions.denied {
        return Err(JsDiagnostic::new(
            "KV6003",
            reason,
            Some(location(
                name,
                source,
                span.lo.0.saturating_sub(base) as usize,
                span.hi.0.saturating_sub(base) as usize,
            )),
            "将模块改为本地静态 ESM 纯计算函数；浏览器组件必须放入客户端模块",
        ));
    }
    Ok(module)
}

pub(super) fn text(value: &ast::Str) -> Result<String, JsDiagnostic> {
    value
        .value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| failure("模块名称包含无法无损转换的 UTF-16 字符"))
}

pub(super) fn imports(module: &ast::Module) -> Result<BTreeSet<String>, JsDiagnostic> {
    let mut result = BTreeSet::new();
    for item in &module.body {
        let ast::ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        let source = match decl {
            ast::ModuleDecl::Import(import) => Some(&import.src),
            ast::ModuleDecl::ExportNamed(export) => export.src.as_ref(),
            ast::ModuleDecl::ExportAll(export) => Some(&export.src),
            _ => None,
        };
        if let Some(source) = source {
            result.insert(text(source)?);
        }
    }
    Ok(result)
}
