use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;

use crate::domain::ir::{
    AttributeIR, AttributeKind, AttributeValueIR, ElementIR, ElementNameIR, IdentifierIR, NodeIR,
    NodeId, NodeKind,
};

use super::Converter;

impl Converter<'_> {
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

    pub(super) fn element(&mut self, element: &ast::JSXElement) -> NodeId {
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

    pub(super) fn fragment(&mut self, fragment: &ast::JSXFragment) -> NodeId {
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
}
