//! 阶段 00 的有限技术探针，测试类型不是正式公共 IR 或 Engine API。

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use swc_common::{FileName, SourceFile, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use thiserror::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, Error)]
enum ProbeError {
    #[error("{path}:{line}:{column}：语法错误：{message}")]
    Syntax {
        path: String,
        line: usize,
        column: usize,
        message: String,
    },
    #[error("{importer} 无法解析模块 {specifier}")]
    Module { importer: String, specifier: String },
    #[error("读取 {path} 失败：{source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Location {
    start: usize,
    end: usize,
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ExpressionRef {
    source: String,
    kind: String,
    symbol: Option<String>,
    location: Location,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Value {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Expression(ExpressionRef),
    Node(Box<Node>),
    Empty(Location),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Attribute {
    Named { name: String, value: Value },
    Spread(ExpressionRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Node {
    Element {
        name: String,
        attributes: Vec<Attribute>,
        children: Vec<Node>,
        location: Location,
    },
    Fragment(Vec<Node>, Location),
    Text(String, Location),
    Expression(ExpressionRef),
    Empty(Location),
    Spread(ExpressionRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Import {
    source: String,
    bindings: Vec<String>,
    type_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ProbeModule {
    format_version: u32,
    path: String,
    imports: Vec<Import>,
    exports: Vec<String>,
    root: Option<Node>,
}

struct Parsed {
    source_map: Lrc<SourceMap>,
    file: Lrc<SourceFile>,
    module: Module,
}

impl Parsed {
    fn parse(path: &str, source: &str, syntax: Syntax) -> Result<Self, ProbeError> {
        let source_map: Lrc<SourceMap> = Default::default();
        let file =
            source_map.new_source_file(FileName::Custom(path.into()).into(), source.to_owned());
        let mut recovered = Vec::new();
        let result = parse_file_as_module(&file, syntax, EsVersion::EsNext, None, &mut recovered);
        // SWC 返回成功 AST 时仍可能有恢复错误，两种错误必须一起检查。
        let module = match result {
            Ok(module) if recovered.is_empty() => module,
            Ok(_) => return Err(syntax_error(&source_map, path, &recovered[0])),
            Err(error) => return Err(syntax_error(&source_map, path, &error)),
        };
        Ok(Self {
            source_map,
            file,
            module,
        })
    }

    fn location(&self, span: Span) -> Location {
        let position = self.source_map.lookup_char_pos(span.lo);
        let start = (span.lo.0 - self.file.start_pos.0) as usize;
        let line_start = self.file.src[..start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        Location {
            start,
            end: (span.hi.0 - self.file.start_pos.0) as usize,
            line: position.line,
            column: self.file.src[line_start..start].chars().count() + 1,
        }
    }

    fn expression(&self, expression: &Expr) -> ExpressionRef {
        let location = self.location(expression.span());
        let (kind, symbol) = match expression {
            Expr::Ident(value) => ("Identifier", Some(value.sym.to_string())),
            Expr::Call(_) => ("Call", None),
            Expr::Cond(_) => ("Conditional", None),
            Expr::Arrow(_) => ("ArrowFunction", None),
            _ => ("OtherExpression", None),
        };
        ExpressionRef {
            source: self.file.src[location.start..location.end].into(),
            kind: kind.into(),
            symbol,
            location,
        }
    }

    fn value(&self, expression: &Expr) -> Value {
        match expression {
            Expr::Lit(Lit::Str(value)) => Value::String(value.value.to_string_lossy().into()),
            Expr::Lit(Lit::Num(value)) => Value::Number(value.value),
            Expr::Lit(Lit::Bool(value)) => Value::Boolean(value.value),
            Expr::Lit(Lit::Null(_)) => Value::Null,
            Expr::JSXElement(element) => Value::Node(Box::new(self.element(element))),
            Expr::JSXFragment(fragment) => Value::Node(Box::new(self.fragment(fragment))),
            _ => Value::Expression(self.expression(expression)),
        }
    }

    fn attribute(&self, attribute: &JSXAttrOrSpread) -> Attribute {
        match attribute {
            JSXAttrOrSpread::SpreadElement(spread) => {
                Attribute::Spread(self.expression(&spread.expr))
            }
            JSXAttrOrSpread::JSXAttr(attribute) => {
                let name = match &attribute.name {
                    JSXAttrName::Ident(name) => name.sym.to_string(),
                    JSXAttrName::JSXNamespacedName(name) => {
                        format!("{}:{}", name.ns.sym, name.name.sym)
                    }
                };
                let value = match &attribute.value {
                    None => Value::Boolean(true),
                    Some(JSXAttrValue::Str(value)) => {
                        Value::String(value.value.to_string_lossy().into())
                    }
                    Some(JSXAttrValue::JSXExprContainer(container)) => match &container.expr {
                        JSXExpr::Expr(expression) => self.value(expression),
                        JSXExpr::JSXEmptyExpr(empty) => Value::Empty(self.location(empty.span)),
                    },
                    Some(JSXAttrValue::JSXElement(element)) => {
                        Value::Node(Box::new(self.element(element)))
                    }
                    Some(JSXAttrValue::JSXFragment(fragment)) => {
                        Value::Node(Box::new(self.fragment(fragment)))
                    }
                };
                Attribute::Named { name, value }
            }
        }
    }

    fn children(&self, children: &[JSXElementChild]) -> Vec<Node> {
        children
            .iter()
            .map(|child| match child {
                JSXElementChild::JSXElement(element) => self.element(element),
                JSXElementChild::JSXFragment(fragment) => self.fragment(fragment),
                JSXElementChild::JSXText(text) => Node::Text(
                    text.value.to_string_lossy().into(),
                    self.location(text.span),
                ),
                JSXElementChild::JSXExprContainer(container) => match &container.expr {
                    JSXExpr::Expr(expression) => Node::Expression(self.expression(expression)),
                    JSXExpr::JSXEmptyExpr(empty) => Node::Empty(self.location(empty.span)),
                },
                JSXElementChild::JSXSpreadChild(child) => {
                    Node::Spread(self.expression(&child.expr))
                }
            })
            .collect()
    }

    fn element(&self, element: &JSXElement) -> Node {
        let name = match &element.opening.name {
            JSXElementName::Ident(name) => name.sym.to_string(),
            JSXElementName::JSXMemberExpr(name) => member_name(name),
            JSXElementName::JSXNamespacedName(name) => format!("{}:{}", name.ns.sym, name.name.sym),
        };
        Node::Element {
            name,
            attributes: element
                .opening
                .attrs
                .iter()
                .map(|attribute| self.attribute(attribute))
                .collect(),
            children: self.children(&element.children),
            location: self.location(element.span),
        }
    }

    fn fragment(&self, fragment: &JSXFragment) -> Node {
        Node::Fragment(
            self.children(&fragment.children),
            self.location(fragment.span),
        )
    }

    fn jsx(&self, expression: &Expr) -> Option<Node> {
        match expression {
            Expr::Paren(expression) => self.jsx(&expression.expr),
            Expr::JSXElement(element) => Some(self.element(element)),
            Expr::JSXFragment(fragment) => Some(self.fragment(fragment)),
            _ => None,
        }
    }

    fn convert(&self, path: &str) -> ProbeModule {
        let mut converted = ProbeModule {
            format_version: 1,
            path: path.into(),
            imports: vec![],
            exports: vec![],
            root: None,
        };
        for item in &self.module.body {
            if let ModuleItem::ModuleDecl(declaration) = item {
                match declaration {
                    ModuleDecl::Import(import) => converted.imports.push(Import {
                        source: import.src.value.to_string_lossy().into(),
                        bindings: import
                            .specifiers
                            .iter()
                            .map(|specifier| specifier.local().sym.to_string())
                            .collect(),
                        type_only: import.type_only,
                    }),
                    ModuleDecl::ExportDefaultExpr(export) => {
                        converted.exports.push("default".into());
                        converted.root = self.jsx(&export.expr);
                    }
                    ModuleDecl::ExportDecl(export) => match &export.decl {
                        Decl::Fn(function) => {
                            converted.exports.push(function.ident.sym.to_string())
                        }
                        Decl::Var(variable) => {
                            for declaration in &variable.decls {
                                if let Pat::Ident(binding) = &declaration.name {
                                    converted.exports.push(binding.id.sym.to_string());
                                }
                            }
                        }
                        Decl::TsTypeAlias(alias) => {
                            converted.exports.push(alias.id.sym.to_string())
                        }
                        _ => {}
                    },
                    ModuleDecl::ExportNamed(export) => {
                        for specifier in &export.specifiers {
                            if let ExportSpecifier::Named(named) = specifier {
                                converted.exports.push(export_name(
                                    named.exported.as_ref().unwrap_or(&named.orig),
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        converted
    }
}

fn syntax_error(map: &SourceMap, path: &str, error: &swc_ecma_parser::error::Error) -> ProbeError {
    let location = map.lookup_char_pos(error.span().lo);
    let start = (error.span().lo.0 - location.file.start_pos.0) as usize;
    let line_start = location.file.src[..start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    ProbeError::Syntax {
        path: path.into(),
        line: location.line,
        column: location.file.src[line_start..start].chars().count() + 1,
        message: error.kind().msg().into_owned(),
    }
}

fn member_name(member: &JSXMemberExpr) -> String {
    let object = match &member.obj {
        JSXObject::Ident(name) => name.sym.to_string(),
        JSXObject::JSXMemberExpr(member) => member_name(member),
    };
    format!("{object}.{}", member.prop.sym)
}

fn export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(name) => name.sym.to_string(),
        ModuleExportName::Str(name) => name.value.to_string_lossy().into(),
    }
}

fn tsx() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: true,
        ..Default::default()
    })
}

fn analyze_fixture(
    root: &Path,
    entry: &Path,
) -> Result<BTreeMap<PathBuf, ProbeModule>, ProbeError> {
    let mut modules = BTreeMap::new();
    let mut pending = vec![entry.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|source| ProbeError::Read {
            path: path.clone(),
            source,
        })?;
        let parsed = Parsed::parse(&path.to_string_lossy(), &source, tsx())?;
        let module = parsed.convert(&path.to_string_lossy());
        for import in &module.imports {
            let base = path.parent().unwrap_or(root).join(&import.source);
            let target = [base.with_extension("ts"), base.with_extension("tsx")]
                .into_iter()
                .find(|candidate| candidate.is_file());
            let Some(target) = target else {
                return Err(ProbeError::Module {
                    importer: module.path.clone(),
                    specifier: import.source.clone(),
                });
            };
            pending.push(target);
        }
        modules.insert(path, module);
    }
    Ok(modules)
}

#[test]
fn parses_four_real_language_modes() -> TestResult {
    let cases = [
        (
            "a.js",
            "export const value = (x) => x + 1;",
            Syntax::Es(EsSyntax::default()),
        ),
        (
            "a.ts",
            "export function id<T>(x:T):T { return x; }",
            Syntax::Typescript(TsSyntax::default()),
        ),
        (
            "a.jsx",
            "export default <Widget enabled />;",
            Syntax::Es(EsSyntax {
                jsx: true,
                ..Default::default()
            }),
        ),
        (
            "a.tsx",
            "const id = <T,>(x:T):T => x; export default <Widget value={id(3)} />;",
            tsx(),
        ),
    ];
    for (path, source, syntax) in cases {
        Parsed::parse(path, source, syntax)?;
    }
    Ok(())
}

#[test]
fn preserves_fragments_attributes_order_literals_and_function_reference() -> TestResult {
    let parsed = Parsed::parse("main.tsx", include_str!("fixtures/p01/main.tsx"), tsx())?;
    let module = parsed.convert("main.tsx");
    let Some(Node::Fragment(children, _)) = &module.root else {
        panic!("应保留 Fragment");
    };
    let panel = children
        .iter()
        .find(|node| matches!(node, Node::Element { name, .. } if name == "panel"));
    let Some(Node::Element {
        attributes,
        children,
        ..
    }) = panel
    else {
        panic!("应保留普通 JSX 元素");
    };
    assert!(
        matches!(&attributes[0], Attribute::Named { name, value: Value::String(value) } if name == "title" && value == "客户😀")
    );
    assert!(matches!(
        &attributes[1],
        Attribute::Named {
            value: Value::Boolean(true),
            ..
        }
    ));
    assert!(
        matches!(&attributes[2], Attribute::Named { value: Value::Number(value), .. } if *value == 3.0)
    );
    assert!(matches!(
        &attributes[3],
        Attribute::Named {
            value: Value::Boolean(false),
            ..
        }
    ));
    assert!(
        matches!(&attributes[4], Attribute::Spread(value) if value.symbol.as_deref() == Some("attributes"))
    );
    assert!(
        matches!(&attributes[5], Attribute::Named { value: Value::Expression(value), .. } if value.symbol.as_deref() == Some("discount"))
    );
    assert!(
        children
            .iter()
            .any(|node| matches!(node, Node::Element { name, .. } if name == "Card"))
    );
    assert!(children.iter().any(|node| matches!(node, Node::Expression(value) if value.kind == "Conditional" && value.source.contains("amount > 0"))));
    let serialized = serde_json::to_string(&module)?;
    assert_eq!(module, serde_json::from_str::<ProbeModule>(&serialized)?);
    Ok(())
}

#[test]
fn associates_imports_exports_and_real_local_modules() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/p01");
    let graph = analyze_fixture(&root, &root.join("main.tsx"))?;
    assert_eq!(graph.len(), 3);
    let entry = &graph[&root.join("main.tsx")];
    assert!(
        entry
            .imports
            .iter()
            .any(|import| import.type_only && import.bindings == ["Amount"])
    );
    for export in ["amount", "identity", "calculateDiscount", "default"] {
        assert!(entry.exports.iter().any(|name| name == export));
    }
    assert!(
        entry
            .imports
            .iter()
            .any(|import| import.bindings == ["Card"] && import.source == "./ui")
    );
    Ok(())
}

#[test]
fn retains_member_names_nested_attribute_nodes_and_unknown_expressions() -> TestResult {
    let parsed = Parsed::parse(
        "names.tsx",
        "export default <UI.Card xml:lang=\"zh\" icon={<Icon />} value={values.map(x => x)}>{...items}</UI.Card>;",
        tsx(),
    )?;
    let Some(Node::Element {
        name,
        attributes,
        children,
        ..
    }) = parsed.convert("names.tsx").root
    else {
        panic!("应保留 Element");
    };
    assert_eq!(name, "UI.Card");
    assert!(matches!(&attributes[0], Attribute::Named { name, .. } if name == "xml:lang"));
    assert!(matches!(
        &attributes[1],
        Attribute::Named {
            value: Value::Node(_),
            ..
        }
    ));
    assert!(
        matches!(&attributes[2], Attribute::Named { value: Value::Expression(value), .. } if value.source == "values.map(x => x)")
    );
    assert!(
        matches!(&children[0], Node::Spread(value) if value.symbol.as_deref() == Some("items"))
    );
    Ok(())
}

#[test]
fn tracks_unicode_columns_crlf_and_half_open_byte_ranges() -> TestResult {
    let source = "// 中文😀\r\nexport default <panel>中文😀<Widget /></panel>;\r\n";
    let parsed = Parsed::parse("位置.tsx", source, tsx())?;
    let Some(Node::Element { children, .. }) = parsed.convert("位置.tsx").root else {
        panic!("应保留根节点");
    };
    let Node::Element { location, .. } = &children[1] else {
        panic!("应保留组件位置");
    };
    assert_eq!(&source[location.start..location.end], "<Widget />");
    assert_eq!(location.line, 2);
    let line_start = source.find("export").ok_or("缺少位置基准")?;
    assert_eq!(
        location.column,
        source[line_start..location.start].chars().count() + 1
    );
    Ok(())
}

#[test]
fn rejects_fatal_and_recovered_syntax_errors_with_context() -> TestResult {
    let error = Parsed::parse("broken.tsx", "export default <Widget>;", tsx());
    assert!(
        matches!(error, Err(ProbeError::Syntax { ref path, line: 1, .. }) if path == "broken.tsx")
    );
    let source_map: Lrc<SourceMap> = Default::default();
    let source = "const value = 1; const enum D { a = value b = value }";
    let file = source_map.new_source_file(FileName::Custom("recover.ts".into()).into(), source);
    let mut errors = Vec::new();
    let result = parse_file_as_module(
        &file,
        Syntax::Typescript(TsSyntax::default()),
        EsVersion::EsNext,
        None,
        &mut errors,
    );
    assert!(result.is_ok(), "此夹具应由 SWC 恢复产生 AST");
    assert!(!errors.is_empty(), "成功 AST 不能掩盖恢复错误");
    assert!(
        Parsed::parse(
            "recover.ts",
            source,
            Syntax::Typescript(TsSyntax::default())
        )
        .is_err()
    );
    Ok(())
}
