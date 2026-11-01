use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::domain::ir::{
    BindingPatternIR, BindingPatternKind, BindingPropertyIR, IdentifierIR, PropertyKeyIR,
    VariableDeclaratorIR,
};

use super::Converter;

impl Converter<'_> {
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

    pub(super) fn variable_declarator(
        &mut self,
        declaration: &ast::VarDeclarator,
    ) -> VariableDeclaratorIR {
        VariableDeclaratorIR {
            span: self.span(declaration.span),
            pattern: self.binding_pattern(&declaration.name),
            initializer: declaration
                .init
                .as_ref()
                .map(|value| self.expression(value)),
        }
    }

    pub(super) fn pattern(&self, pattern: &ast::Pat, names: &mut Vec<IdentifierIR>) {
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
