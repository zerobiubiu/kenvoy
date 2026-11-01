use std::path::Path;

use kengine::{
    ExpressionIR, ExpressionId, ExpressionShape, LiteralIR, MemberPropertyIR, ObjectPropertyKind,
};
use serde_json::{Map, Value};

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::{
    Evaluated, Evaluator,
    evaluator::{ModuleSpan, location},
    values::{number, primitive_text, truthy},
};

impl Evaluator<'_> {
    pub(super) fn eval_expr(
        &mut self,
        path: &Path,
        id: ExpressionId,
    ) -> Result<Evaluated, DslDiagnostic> {
        let module = self.module(path)?;
        let expression = module.expression(id).cloned().ok_or_else(|| {
            self.error(
                "KV1104",
                location(module, module.source_span()),
                "表达式引用无效",
                "重新分析源码",
            )
        })?;
        let source = location(module, expression.span.clone());
        self.tick(&source)?;
        if self
            .expressions
            .iter()
            .any(|(module, expression)| module == path && *expression == id)
        {
            return Err(self.error(
                "KV1102",
                source,
                "静态常量存在循环引用",
                "解除常量或模块默认导出的循环引用",
            ));
        }
        self.expressions.push((path.into(), id));
        let result = self.eval_shape(path, &expression, &source);
        self.expressions.pop();
        result
    }

    fn eval_shape(
        &mut self,
        path: &Path,
        expression: &ExpressionIR,
        source: &SourceLocation,
    ) -> Result<Evaluated, DslDiagnostic> {
        let json = match &expression.shape {
            ExpressionShape::Literal => match &expression.literal {
                Some(LiteralIR::Null) => Value::Null,
                Some(LiteralIR::Boolean(value)) => Value::Bool(*value),
                Some(LiteralIR::Number { value: Some(value) }) => {
                    number(*value).ok_or_else(|| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            "静态数值必须有限",
                            "不要使用 NaN 或 Infinity",
                        )
                    })?
                }
                Some(LiteralIR::String(value)) => {
                    let value = value.to_string().map_err(|_| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            "字符串包含无法表示为 Unicode 的 UTF-16 单元",
                            "使用有效 Unicode 字符串",
                        )
                    })?;
                    self.check_collection(value.chars().count(), source)?;
                    Value::String(value)
                }
                _ => {
                    return Err(self.error(
                        "KV1101",
                        source.clone(),
                        "此字面量不能作为 JSON 声明值",
                        "不要在声明属性中使用 BigInt、正则或非有限数值",
                    ));
                }
            },
            ExpressionShape::Identifier => {
                let identifier = expression.direct_identifier.as_ref().ok_or_else(|| {
                    self.error(
                        "KV1104",
                        source.clone(),
                        "标识符缺少通用结构",
                        "重新分析源码",
                    )
                })?;
                let value = self.eval_target(self.identifier_target(path, identifier)?, source)?;
                return Ok(match value {
                    Evaluated::Reference(reference) => Evaluated::Reference(
                        self.imported_reference(path, identifier, None, source)?
                            .unwrap_or(reference),
                    ),
                    other => other,
                });
            }
            ExpressionShape::Wrapper { expression, .. } => {
                return self.eval_expr(path, *expression);
            }
            ExpressionShape::Jsx { node } => {
                return self.expand_node(path, *node).map(Evaluated::Nodes);
            }
            ExpressionShape::Array { elements } => {
                self.check_collection(elements.len(), source)?;
                let mut values = Vec::new();
                for element in elements {
                    let Some(element) = element else {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "静态 JSON 数组不能包含空洞",
                            "显式使用 null，不能将空洞静默转为 null",
                        ));
                    };
                    let value = self.evaluate_value(path, element.expression)?;
                    if element.spread {
                        let Value::Array(items) = value else {
                            return Err(self.error(
                                "KV1105",
                                source.clone(),
                                "数组展开要求静态数组",
                                "将 spread 目标改为可确定的数组",
                            ));
                        };
                        values.extend(items);
                    } else {
                        values.push(value);
                    }
                    self.check_collection(values.len(), source)?;
                }
                Value::Array(values)
            }
            ExpressionShape::Object { properties } => {
                self.check_collection(properties.len(), source)?;
                let mut values = Map::new();
                for property in properties {
                    match &property.kind {
                        ObjectPropertyKind::Value { key, value, .. } => {
                            let key = self.property_name(path, key, source)?;
                            if key == "__proto__" {
                                return Err(self.error(
                                    "KV1101",
                                    source.clone(),
                                    "静态对象不支持原型设置属性",
                                    "使用普通数据键，避免 __proto__",
                                ));
                            }
                            values.insert(key, self.evaluate_value(path, *value)?);
                        }
                        ObjectPropertyKind::Spread { expression } => {
                            let Value::Object(items) = self.evaluate_value(path, *expression)?
                            else {
                                return Err(self.error(
                                    "KV1105",
                                    source.clone(),
                                    "对象展开要求静态对象",
                                    "将 spread 目标改为可确定的数据对象",
                                ));
                            };
                            values.extend(items);
                        }
                        _ => {
                            return Err(self.error(
                                "KV1101",
                                source.clone(),
                                "声明对象不能包含方法、getter 或 setter",
                                "使用静态数据对象，不能通过函数产生声明",
                            ));
                        }
                    }
                    self.check_collection(values.len(), source)?;
                }
                Value::Object(values)
            }
            ExpressionShape::Unary { operator, argument } => {
                let value = self.evaluate_value(path, *argument)?;
                match operator.as_str() {
                    "!" => Value::Bool(!truthy(&value)),
                    "+" | "-" => {
                        let value = value.as_f64().ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "数值一元运算要求数字",
                                "显式声明数字，不依赖 JavaScript 隐式类型转换",
                            )
                        })?;
                        number(if operator == "-" { -value } else { value }).ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "一元运算结果不是有限数字",
                                "调整静态数值",
                            )
                        })?
                    }
                    _ => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "此一元运算不在静态声明范围内",
                            "使用 !、+、- 或直接字面量",
                        ));
                    }
                }
            }
            ExpressionShape::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.evaluate_value(path, *left)?;
                match operator.as_str() {
                    "&&" if !truthy(&left) => left,
                    "||" if truthy(&left) => left,
                    "??" if !left.is_null() => left,
                    "&&" | "||" | "??" => return self.eval_expr(path, *right),
                    _ => {
                        let right = self.evaluate_value(path, *right)?;
                        self.binary(operator, left, right, source)?
                    }
                }
            }
            ExpressionShape::Conditional {
                test,
                consequent,
                alternate,
            } => {
                let value = self.evaluate_value(path, *test)?;
                return self.eval_expr(
                    path,
                    if truthy(&value) {
                        *consequent
                    } else {
                        *alternate
                    },
                );
            }
            ExpressionShape::Member { object, property } => {
                if let Some(target) =
                    self.namespace_member_target(path, *object, property, source)?
                {
                    let value = self.eval_target(target, source)?;
                    if let Evaluated::Reference(reference) = value {
                        let identifier = self
                            .module(path)?
                            .expression(*object)
                            .and_then(|expression| expression.direct_identifier.as_ref())
                            .cloned()
                            .ok_or_else(|| {
                                self.error(
                                    "KV1104",
                                    source.clone(),
                                    "模块成员缺少直接绑定",
                                    "请直接导入可调用导出",
                                )
                            })?;
                        let name = match property {
                            MemberPropertyIR::Identifier(name) => name.name.clone(),
                            MemberPropertyIR::Computed { expression, .. } => {
                                let name = self.evaluate_value(path, *expression)?;
                                self.key_value(name, source)?
                            }
                            MemberPropertyIR::Private(_) => {
                                return Err(self.error(
                                    "KV1104",
                                    source.clone(),
                                    "私有模块成员不可导入",
                                    "请直接导入可调用导出",
                                ));
                            }
                        };
                        return Ok(Evaluated::Reference(
                            self.imported_reference(path, &identifier, Some(&name), source)?
                                .unwrap_or(reference),
                        ));
                    }
                    return Ok(value);
                }
                let value = self.evaluate_value(path, *object)?;
                let key = match property {
                    MemberPropertyIR::Identifier(value) => value.name.clone(),
                    MemberPropertyIR::Computed { expression, .. } => {
                        let value = self.evaluate_value(path, *expression)?;
                        self.key_value(value, source)?
                    }
                    MemberPropertyIR::Private(_) => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "私有成员不是静态声明值",
                            "使用数据对象属性",
                        ));
                    }
                };
                match value {
                    Value::Object(object) => object.get(&key).cloned().ok_or_else(|| {
                        self.error(
                            "KV1101",
                            source.clone(),
                            format!("静态对象没有属性 {key}"),
                            "检查属性名称，缺失不等同 null",
                        )
                    })?,
                    Value::Array(array) if key == "length" => Value::Number(array.len().into()),
                    Value::Array(array) => key
                        .parse::<usize>()
                        .ok()
                        .filter(|index| index.to_string() == key)
                        .and_then(|index| array.get(index))
                        .cloned()
                        .ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "静态数组下标无效",
                                "使用已存在的非负整数下标",
                            )
                        })?,
                    _ => {
                        return Err(self.error(
                            "KV1101",
                            source.clone(),
                            "静态成员读取只支持数据对象和数组",
                            "不要依赖原型方法或动态对象",
                        ));
                    }
                }
            }
            ExpressionShape::Template {
                quasis,
                expressions,
            } => {
                let mut result = String::new();
                for (index, part) in quasis.iter().enumerate() {
                    let value = part
                        .cooked_utf16
                        .as_ref()
                        .and_then(|value| String::from_utf16(value).ok())
                        .ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "模板字符串包含无效 Unicode",
                                "修复转义字符串",
                            )
                        })?;
                    result.push_str(&value);
                    if let Some(expression) = expressions.get(index) {
                        let value = self.evaluate_value(path, *expression)?;
                        result.push_str(&primitive_text(&value).ok_or_else(|| {
                            self.error(
                                "KV1101",
                                source.clone(),
                                "模板插值只允许基本数据值",
                                "不要隐式转换数组或对象",
                            )
                        })?);
                    }
                    self.check_collection(result.chars().count(), source)?;
                }
                Value::String(result)
            }
            ExpressionShape::Opaque => {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    format!("表达式 {} 无法静态求值", expression.kind),
                    "使用静态值；函数调用、map 和任意执行不能生成 Schema",
                ));
            }
        };
        Ok(Evaluated::Json(json))
    }
}
