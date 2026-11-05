//! 有限键集合求值；所有分支均须可证明，不调用项目函数或访问宿主能力。
use kengine::{ExpressionShape, LiteralIR};
use serde_json::{Number, Value};

use super::bindings::{Expression, Resolver, Target};

const MAX_VALUES: usize = 64;
const MAX_TEXT: usize = 4096;

impl Resolver<'_> {
    pub fn values(&mut self, expression: Expression) -> Option<Vec<Value>> {
        if !self.pending_values.insert(expression) {
            return None;
        }
        // const 不代表对象内容不可变；不能用初始对象快照证明运行时翻译键。
        let result = self.values_inner(expression, &mut Vec::new(), &mut 4096);
        self.pending_values.remove(&expression);
        result
    }

    pub fn strings(&mut self, expression: Expression) -> Option<Vec<String>> {
        let mut strings = Vec::new();
        for value in self.values(expression)? {
            let Value::String(value) = value else {
                return None;
            };
            if value.len() > MAX_TEXT {
                return None;
            }
            if !strings.contains(&value) {
                strings.push(value);
            }
        }
        (!strings.is_empty()).then_some(strings)
    }

    pub fn single_string(&mut self, expression: Expression) -> Option<String> {
        let mut strings = self.strings(expression)?;
        (strings.len() == 1).then(|| strings.remove(0))
    }

    pub fn branches(&mut self, expression: Expression) -> Option<bool> {
        let values = self.values(expression)?;
        let first = truthy(values.first()?);
        values
            .iter()
            .all(|value| truthy(value) == first)
            .then_some(first)
    }

    fn values_inner(
        &mut self,
        expression: Expression,
        stack: &mut Vec<Expression>,
        budget: &mut usize,
    ) -> Option<Vec<Value>> {
        if *budget == 0 || stack.len() >= 128 || stack.contains(&expression) {
            return None;
        }
        *budget -= 1;
        stack.push(expression);
        let result = self.expression_values(expression, stack, budget);
        stack.pop();
        result
    }

    fn expression_values(
        &mut self,
        expression: Expression,
        stack: &mut Vec<Expression>,
        budget: &mut usize,
    ) -> Option<Vec<Value>> {
        let source = &self.sources[expression.module];
        let node = source.module.expression(expression.id)?;
        let child = |id| Expression { id, ..expression };
        match &node.shape {
            ExpressionShape::Literal => Some(vec![match node.literal.as_ref()? {
                LiteralIR::String(value) => Value::String(value.to_string().ok()?),
                LiteralIR::Number { value: Some(value) } => {
                    Value::Number(Number::from_f64(*value)?)
                }
                LiteralIR::Boolean(value) => Value::Bool(*value),
                LiteralIR::Null => Value::Null,
                _ => return None,
            }]),
            ExpressionShape::Wrapper {
                expression: inner, ..
            } => self.values_inner(child(*inner), stack, budget),
            ExpressionShape::Identifier => {
                let symbol = source.symbol(expression.id)?;
                if let Some(target) = self.imports.get(&(expression.module, symbol)).copied() {
                    return self.target_values(target, stack, budget);
                }
                self.symbol_values(expression.module, symbol, stack, budget)
            }
            ExpressionShape::Conditional {
                test,
                consequent,
                alternate,
            } => {
                let test = self.values_inner(child(*test), stack, budget);
                let choices = test
                    .as_ref()
                    .map(|values| {
                        (
                            values.iter().any(truthy),
                            values.iter().any(|value| !truthy(value)),
                        )
                    })
                    .unwrap_or((true, true));
                let mut result = Vec::new();
                for (enabled, branch) in [(choices.0, consequent), (choices.1, alternate)] {
                    if enabled {
                        append(
                            &mut result,
                            self.values_inner(child(*branch), stack, budget)?,
                        )?;
                    }
                }
                Some(result)
            }
            ExpressionShape::Template {
                quasis,
                expressions,
            } => {
                let mut result = vec![String::new()];
                for (index, quasi) in quasis.iter().enumerate() {
                    let text = String::from_utf16(quasi.cooked_utf16.as_ref()?).ok()?;
                    for value in &mut result {
                        value.push_str(&text);
                    }
                    if let Some(id) = expressions.get(index) {
                        let values = self.values_inner(child(*id), stack, budget)?;
                        let mut next = Vec::new();
                        for prefix in &result {
                            for value in &values {
                                let value = format!("{prefix}{}", primitive(value)?);
                                if value.len() > MAX_TEXT || next.len() >= MAX_VALUES {
                                    return None;
                                }
                                if !next.contains(&value) {
                                    next.push(value);
                                }
                            }
                        }
                        result = next;
                    }
                    if result.iter().any(|value| value.len() > MAX_TEXT) {
                        return None;
                    }
                }
                Some(result.into_iter().map(Value::String).collect())
            }
            ExpressionShape::Binary {
                operator,
                left,
                right,
            } if operator == "+" => {
                let left = self.values_inner(child(*left), stack, budget)?;
                let right = self.values_inner(child(*right), stack, budget)?;
                let mut result = Vec::new();
                for left in &left {
                    for right in &right {
                        let value = match (left, right) {
                            (Value::String(_), _) | (_, Value::String(_)) => {
                                let text = format!("{}{}", primitive(left)?, primitive(right)?);
                                if text.len() > MAX_TEXT {
                                    return None;
                                }
                                Value::String(text)
                            }
                            (Value::Number(left), Value::Number(right)) => {
                                Value::Number(Number::from_f64(left.as_f64()? + right.as_f64()?)?)
                            }
                            _ => return None,
                        };
                        append(&mut result, vec![value])?;
                    }
                }
                Some(result)
            }
            ExpressionShape::Unary { operator, argument } => {
                let values = self.values_inner(child(*argument), stack, budget)?;
                values
                    .into_iter()
                    .map(|value| {
                        Some(match operator.as_str() {
                            "!" => Value::Bool(!truthy(&value)),
                            "+" | "-" => Value::Number(Number::from_f64(
                                value.as_f64()? * if operator == "-" { -1.0 } else { 1.0 },
                            )?),
                            _ => return None,
                        })
                    })
                    .collect()
            }
            ExpressionShape::Member { object, property } => {
                let name = self.member_name(expression.module, property)?;
                self.namespace_target(child(*object), &name)
                    .and_then(|target| self.target_values(target, stack, budget))
            }
            _ => None,
        }
    }

    fn symbol_values(
        &mut self,
        module: usize,
        symbol: kengine::SymbolId,
        stack: &mut Vec<Expression>,
        budget: &mut usize,
    ) -> Option<Vec<Value>> {
        let source = &self.sources[module];
        if let Some(target) = self.imports.get(&(module, symbol)).copied() {
            return self.target_values(target, stack, budget);
        }
        let definition = source.definitions.get(&symbol)?;
        if !definition.constant || source.writes[symbol.0] {
            return None;
        }
        let mut values = self.values_inner(
            Expression {
                module,
                id: definition.expression,
            },
            stack,
            budget,
        )?;
        for property in &definition.properties {
            let name = self.property_name(module, property)?;
            values = values
                .into_iter()
                .map(|value| value.get(&name).cloned())
                .collect::<Option<_>>()?;
        }
        Some(values)
    }

    fn target_values(
        &mut self,
        target: Target,
        stack: &mut Vec<Expression>,
        budget: &mut usize,
    ) -> Option<Vec<Value>> {
        match target {
            Target::Symbol(module, symbol) => self.symbol_values(module, symbol, stack, budget),
            Target::Expression(expression) => self.values_inner(expression, stack, budget),
            _ => None,
        }
    }

    fn namespace_target(&mut self, expression: Expression, name: &str) -> Option<Target> {
        let super::bindings::Origin::Module(module) = self.origin(expression) else {
            return None;
        };
        self.exports.get(&(module, name)).copied()
    }
}

fn append(target: &mut Vec<Value>, values: Vec<Value>) -> Option<()> {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
        if target.len() > MAX_VALUES {
            return None;
        }
    }
    Some(())
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

fn primitive(value: &Value) -> Option<String> {
    Some(match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.as_f64()?.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".into(),
        _ => return None,
    })
}
