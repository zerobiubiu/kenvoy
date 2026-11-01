use std::path::Path;

use kengine::PropertyKeyIR;
use serde_json::{Number, Value};

use crate::domain::source::{DslDiagnostic, SourceLocation};

use super::Evaluator;

impl Evaluator<'_> {
    pub(super) fn binary(
        &self,
        operator: &str,
        left: Value,
        right: Value,
        source: &SourceLocation,
    ) -> Result<Value, DslDiagnostic> {
        if matches!(operator, "===" | "!==") {
            if left.is_object() || right.is_object() || left.is_array() || right.is_array() {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    "静态比较不模拟对象身份",
                    "仅比较基本数据值",
                ));
            }
            let equal = if left.is_number() && right.is_number() {
                left.as_f64() == right.as_f64()
            } else {
                left == right
            };
            return Ok(Value::Bool(if operator == "!==" { !equal } else { equal }));
        }
        if operator == "+" && left.is_string() && right.is_string() {
            self.check_collection(
                left.as_str()
                    .unwrap_or_default()
                    .chars()
                    .count()
                    .saturating_add(right.as_str().unwrap_or_default().chars().count()),
                source,
            )?;
            return Ok(Value::String(format!(
                "{}{}",
                left.as_str().unwrap_or_default(),
                right.as_str().unwrap_or_default()
            )));
        }
        let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) else {
            return Err(self.error(
                "KV1101",
                source.clone(),
                "静态运算不支持隐式类型转换",
                "运算两侧使用数字，字符串仅支持同类型拼接",
            ));
        };
        let result = match operator {
            "<" => return Ok(Value::Bool(left < right)),
            "<=" => return Ok(Value::Bool(left <= right)),
            ">" => return Ok(Value::Bool(left > right)),
            ">=" => return Ok(Value::Bool(left >= right)),
            "+" => left + right,
            "-" => left - right,
            "*" => left * right,
            "/" => left / right,
            "%" => left % right,
            "**" => left.powf(right),
            _ => {
                return Err(self.error(
                    "KV1101",
                    source.clone(),
                    format!("静态运算不支持 {operator}"),
                    "使用基本算术、严格比较或显式字面量",
                ));
            }
        };
        number(result).ok_or_else(|| {
            self.error(
                "KV1101",
                source.clone(),
                "静态运算产生非有限数字",
                "检查除零、溢出或指数运算",
            )
        })
    }

    pub(super) fn property_name(
        &mut self,
        path: &Path,
        key: &PropertyKeyIR,
        source: &SourceLocation,
    ) -> Result<String, DslDiagnostic> {
        match key {
            PropertyKeyIR::Identifier(identifier) => Ok(identifier.name.clone()),
            PropertyKeyIR::String(value) => value.to_string().map_err(|_| {
                self.error(
                    "KV1101",
                    source.clone(),
                    "属性名包含无效 Unicode",
                    "使用有效字符串",
                )
            }),
            PropertyKeyIR::Number {
                value: Some(value), ..
            } => Ok(value.to_string()),
            PropertyKeyIR::Computed { expression, .. } => {
                let value = self.evaluate_value(path, *expression)?;
                self.key_value(value, source)
            }
            _ => Err(self.error(
                "KV1101",
                source.clone(),
                "此属性名不能用于静态数据对象",
                "使用字符串、数字或可确定的计算属性",
            )),
        }
    }

    pub(super) fn key_value(
        &self,
        value: Value,
        source: &SourceLocation,
    ) -> Result<String, DslDiagnostic> {
        match value {
            Value::String(value) => Ok(value),
            Value::Number(value) => Ok(value.to_string()),
            _ => Err(self.error(
                "KV1101",
                source.clone(),
                "计算属性名必须是字符串或数字",
                "显式提供有效属性名",
            )),
        }
    }
}

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}
pub(super) fn number(value: f64) -> Option<Value> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0 {
        Some(Value::Number((value as i64).into()))
    } else {
        Number::from_f64(value).map(Value::Number)
    }
}
pub(super) fn primitive_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null => Some("null".into()),
        _ => None,
    }
}
