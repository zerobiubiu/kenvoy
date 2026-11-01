use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use swc_common::{FileName, SourceFile, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast as ast;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use swc_ecma_visit::{Visit, VisitWith};

use crate::{source::SourceIndex, *};

pub(crate) fn parse(path: &Path, source: &str, options: ParseOptions) -> ParseOutput {
    let language = match options.language {
        Language::Auto => match path.extension().and_then(|extension| extension.to_str()) {
            Some("ts" | "mts" | "cts") => Language::TypeScript,
            Some("tsx") => Language::Tsx,
            Some("js" | "mjs" | "cjs") => Language::JavaScript,
            Some("jsx") => Language::Jsx,
            _ => {
                return ParseOutput {
                    module: None,
                    diagnostics: vec![Diagnostic {
                        code: "KE0001".into(),
                        category: DiagnosticCategory::Configuration,
                        severity: Severity::Error,
                        path: path.into(),
                        span: None,
                        message: "无法从文件名确定源语言".into(),
                        suggestion:
                            "使用 .js/.jsx/.ts/.tsx 等扩展名，或显式指定 ParseOptions.language"
                                .into(),
                        related: vec![],
                    }],
                };
            }
        },
        language => language,
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
            converter.output.semantic = crate::semantic::analyze(&module, source, &file, prefix);
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

    fn member_parts(&self, member: &ast::JSXMemberExpr, parts: &mut Vec<IdentifierIR>) {
        match &member.obj {
            ast::JSXObject::Ident(name) => {
                parts.push(self.identifier(name.sym.as_ref(), name.span))
            }
            ast::JSXObject::JSXMemberExpr(member) => self.member_parts(member, parts),
        }
        parts.push(self.identifier(member.prop.sym.as_ref(), member.prop.span));
    }

    fn element_name(&self, name: &ast::JSXElementName) -> ElementNameIR {
        match name {
            ast::JSXElementName::Ident(name) => {
                ElementNameIR::Identifier(self.identifier(name.sym.as_ref(), name.span))
            }
            ast::JSXElementName::JSXMemberExpr(member) => {
                let mut parts = vec![];
                self.member_parts(member, &mut parts);
                ElementNameIR::Member {
                    parts,
                    span: self.span(member.span),
                }
            }
            ast::JSXElementName::JSXNamespacedName(name) => self.namespace(name),
        }
    }

    fn namespace(&self, name: &ast::JSXNamespacedName) -> ElementNameIR {
        ElementNameIR::Namespaced {
            namespace: self.identifier(name.ns.sym.as_ref(), name.ns.span),
            name: self.identifier(name.name.sym.as_ref(), name.name.span),
            span: self.span(name.span),
        }
    }

    fn expression(&mut self, expression: &ast::Expr) -> ExpressionId {
        let span = expression.span();
        let key = (span.lo.0, span.hi.0);
        if let Some(id) = self.expressions.get(&key) {
            return *id;
        }
        let id = ExpressionId(self.output.expressions.len());
        let direct_identifier = if let ast::Expr::Ident(name) = expression {
            Some(self.identifier(name.sym.as_ref(), name.span))
        } else {
            None
        };
        let literal = match expression {
            ast::Expr::Lit(ast::Lit::Str(value)) => Some(LiteralIR::String(self.string(value))),
            ast::Expr::Lit(ast::Lit::Num(value)) => Some(LiteralIR::Number {
                value: value.value.is_finite().then_some(value.value),
            }),
            ast::Expr::Lit(ast::Lit::Bool(value)) => Some(LiteralIR::Boolean(value.value)),
            ast::Expr::Lit(ast::Lit::Null(_)) => Some(LiteralIR::Null),
            ast::Expr::Lit(ast::Lit::BigInt(value)) => Some(LiteralIR::BigInt {
                decimal: value.value.to_string(),
            }),
            ast::Expr::Lit(ast::Lit::Regex(value)) => Some(LiteralIR::Regex {
                pattern: value.exp.to_string(),
                flags: value.flags.to_string(),
            }),
            _ => None,
        };
        self.expressions.insert(key, id);
        self.output.expressions.push(ExpressionIR {
            id,
            span: self.span(span),
            kind: expression_kind(expression).into(),
            direct_identifier,
            literal,
            shape: ExpressionShape::Opaque,
            jsx_roots: vec![],
        });
        let shape = self.expression_shape(expression);
        self.output.expressions[id.0].shape = shape;
        let mut collector = ExpressionJsx {
            converter: self,
            roots: vec![],
        };
        expression.visit_children_with(&mut collector);
        let roots = collector.roots;
        self.output.expressions[id.0].jsx_roots = roots;
        id
    }

    fn expression_shape(&mut self, expression: &ast::Expr) -> ExpressionShape {
        match expression {
            ast::Expr::Lit(_) => ExpressionShape::Literal,
            ast::Expr::Ident(_) => ExpressionShape::Identifier,
            ast::Expr::JSXElement(value) => ExpressionShape::Jsx {
                node: self.element(value),
            },
            ast::Expr::JSXFragment(value) => ExpressionShape::Jsx {
                node: self.fragment(value),
            },
            ast::Expr::Paren(value) => {
                self.wrapper(ExpressionWrapperKind::Parenthesized, &value.expr)
            }
            ast::Expr::TsAs(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptAs, &value.expr)
            }
            ast::Expr::TsSatisfies(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptSatisfies, &value.expr)
            }
            ast::Expr::TsNonNull(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptNonNull, &value.expr)
            }
            ast::Expr::TsTypeAssertion(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptTypeAssertion, &value.expr)
            }
            ast::Expr::TsConstAssertion(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptConstAssertion, &value.expr)
            }
            ast::Expr::TsInstantiation(value) => {
                self.wrapper(ExpressionWrapperKind::TypeScriptInstantiation, &value.expr)
            }
            ast::Expr::Array(value) => ExpressionShape::Array {
                elements: value
                    .elems
                    .iter()
                    .map(|item| {
                        item.as_ref().map(|item| ExpressionItemIR {
                            span: self.span(item.span()),
                            expression: self.expression(&item.expr),
                            spread: item.spread.is_some(),
                        })
                    })
                    .collect(),
            },
            ast::Expr::Object(value) => ExpressionShape::Object {
                properties: value
                    .props
                    .iter()
                    .map(|property| self.object_property(property))
                    .collect(),
            },
            ast::Expr::Unary(value) => ExpressionShape::Unary {
                operator: value.op.as_str().into(),
                argument: self.expression(&value.arg),
            },
            ast::Expr::Bin(value) => ExpressionShape::Binary {
                operator: value.op.as_str().into(),
                left: self.expression(&value.left),
                right: self.expression(&value.right),
            },
            ast::Expr::Cond(value) => ExpressionShape::Conditional {
                test: self.expression(&value.test),
                consequent: self.expression(&value.cons),
                alternate: self.expression(&value.alt),
            },
            ast::Expr::Member(value) => ExpressionShape::Member {
                object: self.expression(&value.obj),
                property: match &value.prop {
                    ast::MemberProp::Ident(name) => {
                        MemberPropertyIR::Identifier(self.identifier(name.sym.as_ref(), name.span))
                    }
                    ast::MemberProp::PrivateName(name) => {
                        MemberPropertyIR::Private(self.identifier(name.name.as_ref(), name.span))
                    }
                    ast::MemberProp::Computed(value) => MemberPropertyIR::Computed {
                        span: self.span(value.span),
                        expression: self.expression(&value.expr),
                    },
                },
            },
            ast::Expr::Tpl(value) => ExpressionShape::Template {
                quasis: value
                    .quasis
                    .iter()
                    .map(|part| TemplatePartIR {
                        span: self.span(part.span),
                        raw: part.raw.to_string(),
                        cooked_utf16: part
                            .cooked
                            .as_ref()
                            .map(|value| value.to_ill_formed_utf16().collect()),
                        tail: part.tail,
                    })
                    .collect(),
                expressions: value
                    .exprs
                    .iter()
                    .map(|value| self.expression(value))
                    .collect(),
            },
            _ => ExpressionShape::Opaque,
        }
    }

    fn wrapper(&mut self, kind: ExpressionWrapperKind, expression: &ast::Expr) -> ExpressionShape {
        ExpressionShape::Wrapper {
            kind,
            expression: self.expression(expression),
        }
    }

    fn property_key(&mut self, key: &ast::PropName) -> PropertyKeyIR {
        match key {
            ast::PropName::Ident(name) => {
                PropertyKeyIR::Identifier(self.identifier(name.sym.as_ref(), name.span))
            }
            ast::PropName::Str(value) => PropertyKeyIR::String(self.string(value)),
            ast::PropName::Num(value) => PropertyKeyIR::Number {
                span: self.span(value.span),
                value: value.value.is_finite().then_some(value.value),
            },
            ast::PropName::BigInt(value) => PropertyKeyIR::BigInt {
                span: self.span(value.span),
                decimal: value.value.to_string(),
            },
            ast::PropName::Computed(value) => PropertyKeyIR::Computed {
                span: self.span(value.span),
                expression: self.expression(&value.expr),
            },
        }
    }

    fn object_property(&mut self, property: &ast::PropOrSpread) -> ObjectPropertyIR {
        let kind = match property {
            ast::PropOrSpread::Spread(value) => ObjectPropertyKind::Spread {
                expression: self.expression(&value.expr),
            },
            ast::PropOrSpread::Prop(value) => match value.as_ref() {
                ast::Prop::Shorthand(name) => ObjectPropertyKind::Value {
                    key: PropertyKeyIR::Identifier(self.identifier(name.sym.as_ref(), name.span)),
                    value: self.expression(&ast::Expr::Ident(name.clone())),
                    shorthand: true,
                },
                ast::Prop::KeyValue(value) => ObjectPropertyKind::Value {
                    key: self.property_key(&value.key),
                    value: self.expression(&value.value),
                    shorthand: false,
                },
                ast::Prop::Method(value) => ObjectPropertyKind::Method {
                    key: self.property_key(&value.key),
                    kind: ObjectMethodKind::Method,
                },
                ast::Prop::Getter(value) => ObjectPropertyKind::Method {
                    key: self.property_key(&value.key),
                    kind: ObjectMethodKind::Getter,
                },
                ast::Prop::Setter(value) => ObjectPropertyKind::Method {
                    key: self.property_key(&value.key),
                    kind: ObjectMethodKind::Setter,
                },
                ast::Prop::Assign(_) => ObjectPropertyKind::Opaque,
            },
        };
        ObjectPropertyIR {
            span: self.span(property.span()),
            kind,
        }
    }

    fn binding_pattern(&mut self, pattern: &ast::Pat) -> BindingPatternIR {
        let kind = match pattern {
            ast::Pat::Ident(name) => {
                BindingPatternKind::Identifier(self.identifier(name.id.sym.as_ref(), name.id.span))
            }
            ast::Pat::Array(value) => BindingPatternKind::Array {
                elements: value
                    .elems
                    .iter()
                    .map(|element| {
                        element
                            .as_ref()
                            .map(|element| self.binding_pattern(element))
                    })
                    .collect(),
            },
            ast::Pat::Object(value) => BindingPatternKind::Object {
                properties: value
                    .props
                    .iter()
                    .map(|property| match property {
                        ast::ObjectPatProp::KeyValue(value) => BindingPropertyIR::Property {
                            span: self.span(value.span()),
                            key: self.property_key(&value.key),
                            value: self.binding_pattern(&value.value),
                            shorthand: false,
                        },
                        ast::ObjectPatProp::Assign(value) => {
                            let name =
                                self.identifier(value.key.id.sym.as_ref(), value.key.id.span);
                            let binding = BindingPatternIR {
                                span: self.span(value.key.span()),
                                kind: BindingPatternKind::Identifier(name.clone()),
                            };
                            let binding = if let Some(default) = &value.value {
                                BindingPatternIR {
                                    span: self.span(value.span),
                                    kind: BindingPatternKind::Assignment {
                                        target: Box::new(binding),
                                        default: self.expression(default),
                                    },
                                }
                            } else {
                                binding
                            };
                            BindingPropertyIR::Property {
                                span: self.span(value.span),
                                key: PropertyKeyIR::Identifier(name),
                                value: binding,
                                shorthand: true,
                            }
                        }
                        ast::ObjectPatProp::Rest(value) => BindingPropertyIR::Rest {
                            span: self.span(value.span),
                            target: self.binding_pattern(&value.arg),
                        },
                    })
                    .collect(),
            },
            ast::Pat::Rest(value) => BindingPatternKind::Rest {
                target: Box::new(self.binding_pattern(&value.arg)),
            },
            ast::Pat::Assign(value) => BindingPatternKind::Assignment {
                target: Box::new(self.binding_pattern(&value.left)),
                default: self.expression(&value.right),
            },
            _ => BindingPatternKind::Opaque,
        };
        BindingPatternIR {
            span: self.span(pattern.span()),
            kind,
        }
    }

    fn variable_declarator(&mut self, declaration: &ast::VarDeclarator) -> VariableDeclaratorIR {
        VariableDeclaratorIR {
            span: self.span(declaration.span),
            pattern: self.binding_pattern(&declaration.name),
            initializer: declaration
                .init
                .as_ref()
                .map(|value| self.expression(value)),
        }
    }

    fn insert_node(&mut self, span: Span, category: &'static str) -> (NodeId, bool) {
        let key = (span.lo.0, span.hi.0, category);
        if let Some(id) = self.nodes.get(&key) {
            return (*id, false);
        }
        let id = NodeId(self.output.nodes.len());
        self.nodes.insert(key, id);
        self.output.nodes.push(NodeIR {
            id,
            span: self.span(span),
            kind: NodeKind::Fragment { children: vec![] },
        });
        (id, true)
    }

    fn element(&mut self, element: &ast::JSXElement) -> NodeId {
        let (id, fresh) = self.insert_node(element.span, "element");
        if !fresh {
            return id;
        }
        let attributes = element
            .opening
            .attrs
            .iter()
            .map(|attribute| self.attribute(attribute))
            .collect();
        let children = self.children(&element.children);
        self.output.nodes[id.0].kind = NodeKind::Element(Box::new(ElementIR {
            name: self.element_name(&element.opening.name),
            attributes,
            children,
            self_closing: element.opening.self_closing,
            opening_span: self.span(element.opening.span),
            closing_span: element
                .closing
                .as_ref()
                .map(|closing| self.span(closing.span)),
        }));
        id
    }

    fn fragment(&mut self, fragment: &ast::JSXFragment) -> NodeId {
        let (id, fresh) = self.insert_node(fragment.span, "fragment");
        if fresh {
            let children = self.children(&fragment.children);
            self.output.nodes[id.0].kind = NodeKind::Fragment { children };
        }
        id
    }

    fn children(&mut self, children: &[ast::JSXElementChild]) -> Vec<NodeId> {
        children
            .iter()
            .map(|child| match child {
                ast::JSXElementChild::JSXElement(element) => self.element(element),
                ast::JSXElementChild::JSXFragment(fragment) => self.fragment(fragment),
                ast::JSXElementChild::JSXText(text) => {
                    let (id, _) = self.insert_node(text.span, "text");
                    self.output.nodes[id.0].kind = NodeKind::Text {
                        value_utf16: text.value.to_ill_formed_utf16().collect(),
                    };
                    id
                }
                ast::JSXElementChild::JSXExprContainer(container) => {
                    let expression = match &container.expr {
                        ast::JSXExpr::Expr(expression) => Some(self.expression(expression)),
                        ast::JSXExpr::JSXEmptyExpr(_) => None,
                    };
                    let (id, _) = self.insert_node(container.span, "expression_child");
                    self.output.nodes[id.0].kind = NodeKind::Expression { expression };
                    id
                }
                ast::JSXElementChild::JSXSpreadChild(child) => {
                    let expression = self.expression(&child.expr);
                    let (id, _) = self.insert_node(child.span, "spread_child");
                    self.output.nodes[id.0].kind = NodeKind::Spread { expression };
                    id
                }
            })
            .collect()
    }

    fn attribute(&mut self, attribute: &ast::JSXAttrOrSpread) -> AttributeIR {
        let kind = match attribute {
            ast::JSXAttrOrSpread::SpreadElement(spread) => AttributeKind::Spread {
                expression: self.expression(&spread.expr),
            },
            ast::JSXAttrOrSpread::JSXAttr(attribute) => {
                let name = match &attribute.name {
                    ast::JSXAttrName::Ident(name) => {
                        ElementNameIR::Identifier(self.identifier(name.sym.as_ref(), name.span))
                    }
                    ast::JSXAttrName::JSXNamespacedName(name) => self.namespace(name),
                };
                let value = attribute.value.as_ref().map(|value| match value {
                    ast::JSXAttrValue::Str(value) => AttributeValueIR::String(self.string(value)),
                    ast::JSXAttrValue::JSXElement(element) => {
                        AttributeValueIR::Element(self.element(element))
                    }
                    ast::JSXAttrValue::JSXFragment(fragment) => {
                        AttributeValueIR::Fragment(self.fragment(fragment))
                    }
                    ast::JSXAttrValue::JSXExprContainer(container) => {
                        AttributeValueIR::Expression {
                            expression: match &container.expr {
                                ast::JSXExpr::Expr(expression) => Some(self.expression(expression)),
                                ast::JSXExpr::JSXEmptyExpr(_) => None,
                            },
                            span: self.span(container.span),
                        }
                    }
                });
                AttributeKind::Named {
                    name: Box::new(name),
                    value,
                }
            }
        };
        AttributeIR {
            span: self.span(attribute.span()),
            kind,
        }
    }

    fn module_declarations(&mut self, module: &ast::Module) {
        for item in &module.body {
            let ast::ModuleItem::ModuleDecl(declaration) = item else {
                continue;
            };
            match declaration {
                ast::ModuleDecl::Import(import) => {
                    let specifiers = import
                        .specifiers
                        .iter()
                        .map(|specifier| {
                            let local = specifier.local();
                            let (kind, imported) = match specifier {
                                ast::ImportSpecifier::Default(_) => {
                                    (ImportSpecifierKind::Default, None)
                                }
                                ast::ImportSpecifier::Namespace(_) => {
                                    (ImportSpecifierKind::Namespace, None)
                                }
                                ast::ImportSpecifier::Named(named) => (
                                    ImportSpecifierKind::Named,
                                    Some(
                                        named
                                            .imported
                                            .as_ref()
                                            .map(|name| self.module_name(name))
                                            .unwrap_or_else(|| {
                                                ModuleNameIR::Identifier(
                                                    self.identifier(local.sym.as_ref(), local.span),
                                                )
                                            }),
                                    ),
                                ),
                            };
                            ImportSpecifierIR {
                                span: self.span(specifier.span()),
                                kind,
                                imported,
                                local: self.identifier(local.sym.as_ref(), local.span),
                                type_only: import.type_only || specifier.is_type_only(),
                            }
                        })
                        .collect();
                    self.output.imports.push(ImportIR {
                        span: self.span(import.span),
                        source: Some(self.string(&import.src)),
                        specifiers,
                        type_only: import.type_only,
                        attributes: import.with.as_ref().map(|value| self.span(value.span)),
                        typescript_module_reference: None,
                    });
                }
                ast::ModuleDecl::TsImportEquals(import) => {
                    let source = match &import.module_ref {
                        ast::TsModuleRef::TsExternalModuleRef(value) => {
                            Some(self.string(&value.expr))
                        }
                        _ => None,
                    };
                    self.output.imports.push(ImportIR {
                        span: self.span(import.span),
                        source,
                        specifiers: vec![ImportSpecifierIR {
                            span: self.span(import.id.span),
                            kind: ImportSpecifierKind::TypeScriptEquals,
                            local: self.identifier(import.id.sym.as_ref(), import.id.span),
                            imported: None,
                            type_only: import.is_type_only,
                        }],
                        type_only: import.is_type_only,
                        attributes: None,
                        typescript_module_reference: Some(self.span(import.module_ref.span())),
                    });
                    if import.is_export {
                        self.output.exports.push(ExportIR {
                            span: self.span(import.span),
                            kind: ExportKind::Named,
                            source: None,
                            specifiers: vec![ExportSpecifierIR {
                                span: self.span(import.id.span),
                                kind: ExportSpecifierKind::Named,
                                local: Some(ModuleNameIR::Identifier(
                                    self.identifier(import.id.sym.as_ref(), import.id.span),
                                )),
                                exported: ModuleNameIR::Identifier(
                                    self.identifier(import.id.sym.as_ref(), import.id.span),
                                ),
                                type_only: import.is_type_only,
                            }],
                            type_only: import.is_type_only,
                            attributes: None,
                        });
                    }
                }
                ast::ModuleDecl::ExportDecl(export) => {
                    let span = export.decl.span();
                    self.exported.insert((span.lo.0, span.hi.0));
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::Declaration {
                            declaration_span: self.span(span),
                        },
                        source: None,
                        specifiers: vec![],
                        type_only: matches!(
                            export.decl,
                            ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_)
                        ),
                        attributes: None,
                    });
                }
                ast::ModuleDecl::ExportNamed(export) => {
                    let specifiers = export
                        .specifiers
                        .iter()
                        .map(|specifier| match specifier {
                            ast::ExportSpecifier::Named(named) => ExportSpecifierIR {
                                span: self.span(named.span),
                                kind: ExportSpecifierKind::Named,
                                local: Some(self.module_name(&named.orig)),
                                exported: self
                                    .module_name(named.exported.as_ref().unwrap_or(&named.orig)),
                                type_only: export.type_only || named.is_type_only,
                            },
                            ast::ExportSpecifier::Namespace(namespace) => ExportSpecifierIR {
                                span: self.span(namespace.span),
                                kind: ExportSpecifierKind::Namespace,
                                local: None,
                                exported: self.module_name(&namespace.name),
                                type_only: export.type_only,
                            },
                            ast::ExportSpecifier::Default(default) => ExportSpecifierIR {
                                span: self.span(default.exported.span),
                                kind: ExportSpecifierKind::Default,
                                local: None,
                                exported: ModuleNameIR::Identifier(self.identifier(
                                    default.exported.sym.as_ref(),
                                    default.exported.span,
                                )),
                                type_only: export.type_only,
                            },
                        })
                        .collect();
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::Named,
                        source: export.src.as_ref().map(|source| self.string(source)),
                        specifiers,
                        type_only: export.type_only,
                        attributes: export.with.as_ref().map(|value| self.span(value.span)),
                    });
                }
                ast::ModuleDecl::ExportAll(export) => self.output.exports.push(ExportIR {
                    span: self.span(export.span),
                    kind: ExportKind::All,
                    source: Some(self.string(&export.src)),
                    specifiers: vec![],
                    type_only: export.type_only,
                    attributes: export.with.as_ref().map(|value| self.span(value.span)),
                }),
                ast::ModuleDecl::ExportDefaultExpr(export) => {
                    let expression = self.expression(&export.expr);
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::DefaultExpression { expression },
                        source: None,
                        specifiers: vec![],
                        type_only: false,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::ExportDefaultDecl(export) => {
                    let (kind, name) = match &export.decl {
                        ast::DefaultDecl::Fn(value) => {
                            (DeclarationKind::Function, value.ident.as_ref())
                        }
                        ast::DefaultDecl::Class(value) => {
                            (DeclarationKind::Class, value.ident.as_ref())
                        }
                        ast::DefaultDecl::TsInterfaceDecl(value) => {
                            (DeclarationKind::Interface, Some(&value.id))
                        }
                    };
                    let name = name.map(|name| self.identifier(name.sym.as_ref(), name.span));
                    let span = self.span(export.decl.span());
                    self.output.declarations.push(DeclarationIR {
                        span: span.clone(),
                        kind,
                        bindings: name.iter().cloned().collect(),
                        exported: true,
                        declarators: vec![],
                    });
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::DefaultDeclaration {
                            declaration_span: span,
                            name,
                        },
                        source: None,
                        specifiers: vec![],
                        type_only: kind == DeclarationKind::Interface,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::TsExportAssignment(export) => {
                    let expression = self.expression(&export.expr);
                    self.output.exports.push(ExportIR {
                        span: self.span(export.span),
                        kind: ExportKind::TypeScriptAssignment { expression },
                        source: None,
                        specifiers: vec![],
                        type_only: false,
                        attributes: None,
                    });
                }
                ast::ModuleDecl::TsNamespaceExport(export) => self.output.exports.push(ExportIR {
                    span: self.span(export.span),
                    kind: ExportKind::TypeScriptNamespace {
                        name: self.identifier(export.id.sym.as_ref(), export.id.span),
                    },
                    source: None,
                    specifiers: vec![],
                    type_only: false,
                    attributes: None,
                }),
            }
        }
    }

    fn pattern(&self, pattern: &ast::Pat, names: &mut Vec<IdentifierIR>) {
        match pattern {
            ast::Pat::Ident(name) => {
                names.push(self.identifier(name.id.sym.as_ref(), name.id.span))
            }
            ast::Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.pattern(element, names);
                }
            }
            ast::Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        ast::ObjectPatProp::KeyValue(value) => self.pattern(&value.value, names),
                        ast::ObjectPatProp::Assign(value) => names
                            .push(self.identifier(value.key.id.sym.as_ref(), value.key.id.span)),
                        ast::ObjectPatProp::Rest(value) => self.pattern(&value.arg, names),
                    }
                }
            }
            ast::Pat::Assign(value) => self.pattern(&value.left, names),
            ast::Pat::Rest(value) => self.pattern(&value.arg, names),
            _ => {}
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

struct ExpressionJsx<'a, 'b> {
    converter: &'a mut Converter<'b>,
    roots: Vec<NodeId>,
}

impl Visit for ExpressionJsx<'_, '_> {
    fn visit_expr(&mut self, expression: &ast::Expr) {
        // 子表达式先建立并复用摘要，避免对每个父表达式重复扫描整棵 AST。
        let id = self.converter.expression(expression);
        self.roots.extend(
            self.converter.output.expressions[id.0]
                .jsx_roots
                .iter()
                .copied(),
        );
    }
    fn visit_jsx_element(&mut self, element: &ast::JSXElement) {
        self.roots.push(self.converter.element(element));
    }
    fn visit_jsx_fragment(&mut self, fragment: &ast::JSXFragment) {
        self.roots.push(self.converter.fragment(fragment));
    }
}

fn expression_kind(expression: &ast::Expr) -> &'static str {
    match expression {
        ast::Expr::Ident(_) => "Identifier",
        ast::Expr::Lit(_) => "Literal",
        ast::Expr::Call(_) => "Call",
        ast::Expr::Cond(_) => "Conditional",
        ast::Expr::Arrow(_) => "ArrowFunction",
        ast::Expr::Fn(_) => "Function",
        ast::Expr::Class(_) => "Class",
        ast::Expr::JSXElement(_) => "JSXElement",
        ast::Expr::JSXFragment(_) => "JSXFragment",
        ast::Expr::Paren(_) => "Parenthesized",
        ast::Expr::Member(_) => "Member",
        ast::Expr::Bin(_) => "Binary",
        ast::Expr::Unary(_) => "Unary",
        ast::Expr::Assign(_) => "Assignment",
        ast::Expr::Array(_) => "Array",
        ast::Expr::Object(_) => "Object",
        ast::Expr::Await(_) => "Await",
        ast::Expr::Yield(_) => "Yield",
        ast::Expr::New(_) => "New",
        ast::Expr::Tpl(_) => "Template",
        ast::Expr::TsAs(_) => "TypeScriptAs",
        ast::Expr::TsSatisfies(_) => "TypeScriptSatisfies",
        ast::Expr::TsNonNull(_) => "TypeScriptNonNull",
        _ => "OtherExpression",
    }
}
