//! 仅有 count 的调用可以读取 .other；已知非数值 count 不进入复数查词。
use kengine::{ExpressionShape, ObjectPropertyKind, SymbolId};
use serde_json::Value;

use super::bindings::{Expression, Resolver, Target};

impl Resolver<'_> {
    pub fn plural(&mut self, expression: Expression) -> bool {
        self.plural_inner(expression, &mut Vec::new())
    }

    fn plural_inner(&mut self, expression: Expression, stack: &mut Vec<Expression>) -> bool {
        if stack.len() >= 128 || stack.contains(&expression) {
            return false;
        }
        stack.push(expression);
        let result = self.expression_plural(expression, stack);
        stack.pop();
        result
    }

    fn expression_plural(&mut self, expression: Expression, stack: &mut Vec<Expression>) -> bool {
        let source = &self.sources[expression.module];
        let Some(node) = source.module.expression(expression.id) else {
            return false;
        };
        let child = |id| Expression { id, ..expression };
        match &node.shape {
            ExpressionShape::Wrapper {
                expression: inner, ..
            } => self.plural_inner(child(*inner), stack),
            ExpressionShape::Identifier => source
                .symbol(expression.id)
                .is_some_and(|symbol| self.symbol_plural(expression.module, symbol, stack)),
            ExpressionShape::Object { properties } => {
                for property in properties.iter().rev() {
                    let ObjectPropertyKind::Value { key, value, .. } = &property.kind else {
                        return false;
                    };
                    match self.property_name(expression.module, key).as_deref() {
                        Some("count") => return self.numeric_count(child(*value)),
                        Some(_) => {}
                        None => return false,
                    }
                }
                false
            }
            ExpressionShape::Conditional {
                test,
                consequent,
                alternate,
            } => match self.branches(child(*test)) {
                Some(true) => self.plural_inner(child(*consequent), stack),
                Some(false) => self.plural_inner(child(*alternate), stack),
                None => {
                    self.plural_inner(child(*consequent), stack)
                        && self.plural_inner(child(*alternate), stack)
                }
            },
            _ => self.values(expression).is_some_and(|values| {
                values
                    .iter()
                    .all(|value| value.get("count").is_some_and(Value::is_number))
            }),
        }
    }

    fn symbol_plural(
        &mut self,
        module: usize,
        symbol: SymbolId,
        stack: &mut Vec<Expression>,
    ) -> bool {
        if let Some(target) = self.imports.get(&(module, symbol)).copied() {
            return match target {
                Target::Symbol(module, symbol) => self.symbol_plural(module, symbol, stack),
                Target::Expression(expression) => self.plural_inner(expression, stack),
                _ => false,
            };
        }
        let source = &self.sources[module];
        source
            .definitions
            .get(&symbol)
            .filter(|definition| {
                definition.constant && !source.writes[symbol.0] && definition.properties.is_empty()
            })
            .is_some_and(|definition| {
                self.plural_inner(
                    Expression {
                        module,
                        id: definition.expression,
                    },
                    stack,
                )
            })
    }

    fn numeric_count(&mut self, expression: Expression) -> bool {
        if let Some(values) = self.values(expression) {
            return values.iter().all(Value::is_number);
        }
        let source = &self.sources[expression.module];
        let Some(node) = source.module.expression(expression.id) else {
            return false;
        };
        match &node.shape {
            ExpressionShape::Wrapper {
                expression: inner, ..
            } => self.numeric_count(Expression {
                id: *inner,
                ..expression
            }),
            ExpressionShape::Literal
            | ExpressionShape::Template { .. }
            | ExpressionShape::Array { .. }
            | ExpressionShape::Object { .. } => false,
            ExpressionShape::Identifier if source.symbol(expression.id).is_none() => !node
                .direct_identifier
                .as_ref()
                .is_some_and(|name| matches!(name.name.as_str(), "undefined" | "NaN" | "Infinity")),
            _ => !matches!(node.kind.as_str(), "Function" | "ArrowFunction" | "Class"),
        }
    }
}
