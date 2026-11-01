use std::{collections::BTreeSet, fmt};

use serde::{
    Deserialize,
    de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Number, Value};

use crate::domain::model::ResourceDefinition;

use super::{ValidationError, ValidationErrors, validate_field};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Create,
    Patch,
}

/// Create 为缺失字段应用默认值或 null；Patch 只验证显式提供的字段，不应用默认值。
/// 未知字段（包括 id 等系统字段）拒绝。授权字段投影由调用方另行检查，不能混为字段验证。
pub fn validate_record(
    resource: &ResourceDefinition,
    input: &Value,
    mode: WriteMode,
) -> Result<Map<String, Value>, ValidationErrors> {
    let Some(object) = input.as_object() else {
        return Err(ValidationErrors(vec![ValidationError {
            code: "KV4010".into(),
            field: None,
            message: "记录必须为 JSON 对象".into(),
        }]));
    };
    let mut errors = Vec::new();
    for name in object.keys() {
        if resource.field(name).is_none() {
            errors.push(ValidationError {
                code: "KV4011".into(),
                field: Some(name.clone()),
                message: "字段未在资源中声明或属于只读系统字段".into(),
            });
        }
    }
    let mut output = Map::new();
    for field in &resource.fields {
        let value = match object.get(&field.name) {
            Some(value) => value,
            None if mode == WriteMode::Patch => continue,
            None => field.default.as_ref().unwrap_or(&Value::Null),
        };
        match validate_field(field, value) {
            Ok(value) => {
                output.insert(field.name.clone(), value);
            }
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(output)
    } else {
        Err(ValidationErrors(errors))
    }
}

/// 在任意深度的 JSON 对象丢失重复字段之前拒绝参数污染。
/// 标准语法、数字解析与递归限制仍交给 serde_json。
pub fn parse_record_json(input: &[u8]) -> Result<Value, ValidationError> {
    struct Record(Value);
    struct RecordVisitor;
    impl<'de> Visitor<'de> for RecordVisitor {
        type Value = Record;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("不含重复字段的 JSON 记录对象")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut object = Map::new();
            let mut keys = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key.clone()) {
                    return Err(de::Error::custom("记录包含重复字段"));
                }
                let value = map.next_value::<Record>()?;
                object.insert(key, value.0);
            }
            Ok(Record(Value::Object(object)))
        }
        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = vec![];
            while let Some(value) = sequence.next_element::<Record>()? {
                values.push(value.0);
            }
            Ok(Record(Value::Array(values)))
        }
        fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
            Ok(Record(Value::Bool(value)))
        }
        fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Record(Value::Number(value.into())))
        }
        fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Record(Value::Number(value.into())))
        }
        fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
            Number::from_f64(value)
                .map(|value| Record(Value::Number(value)))
                .ok_or_else(|| E::custom("JSON 数字必须为有限值"))
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Record(Value::String(value.into())))
        }
        fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
            Ok(Record(Value::String(value)))
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Record(Value::Null))
        }
    }
    impl<'de> Deserialize<'de> for Record {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(RecordVisitor)
        }
    }
    serde_json::from_slice::<Record>(input)
        .and_then(|record| {
            if record.0.is_object() {
                Ok(record.0)
            } else {
                Err(<serde_json::Error as de::Error>::custom(
                    "请求必须为 JSON 对象",
                ))
            }
        })
        .map_err(|_| ValidationError {
            code: "KV4012".into(),
            field: None,
            message: "请求必须为有效且不含重复字段的 JSON 对象".into(),
        })
}
