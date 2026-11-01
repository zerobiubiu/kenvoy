use swc_common::Spanned;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

use crate::domain::ir::{
    ExpressionIR, ExpressionId, ExpressionItemIR, ExpressionShape, ExpressionWrapperKind,
    LiteralIR, MemberPropertyIR, NodeId, ObjectMethodKind, ObjectPropertyIR, ObjectPropertyKind,
    PropertyKeyIR, TemplatePartIR,
};

use super::Converter;

impl Converter<'_> {
    pub(super) fn expression(&mut self, expression: &ast::Expr) -> ExpressionId {
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

    pub(super) fn property_key(&mut self, key: &ast::PropName) -> PropertyKeyIR {
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
