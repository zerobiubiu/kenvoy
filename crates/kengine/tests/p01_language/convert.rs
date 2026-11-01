use super::*;

impl Parsed {
    pub(super) fn parse(path: &str, source: &str, syntax: Syntax) -> Result<Self, ProbeError> {
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

    pub(super) fn convert(&self, path: &str) -> ProbeModule {
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
