//! 与传输层无关的字段约束；DSL 默认值、资源写入和浏览器规则共享同一契约。

use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Number, Value};

use crate::application::{FieldDefinition, FieldKind, ResourceDefinition};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub field: Option<String>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{} 字段 {field}：{}", self.code, self.message),
            None => write!(f, "{}：{}", self.code, self.message),
        }
    }
}
impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationErrors(pub Vec<ValidationError>);

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, "；")?
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ValidationErrors {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Create,
    Patch,
}

fn field_error(field: &FieldDefinition, code: &str, message: &str) -> ValidationError {
    ValidationError {
        code: code.into(),
        field: Some(field.name.clone()),
        message: message.into(),
    }
}

/// 验证并规范化单个显式值。可空性由 !required 决定；不做隐式字符串/布尔转换。
pub fn validate_field(field: &FieldDefinition, value: &Value) -> Result<Value, ValidationError> {
    if value.as_str().is_some_and(|text| text.contains('\0')) {
        return Err(field_error(field, "KV4002", "字段文本不能包含 NUL 字符"));
    }
    if value.is_null() {
        return if field.required {
            Err(field_error(field, "KV4001", "必填字段不能为 null"))
        } else {
            Ok(Value::Null)
        };
    }
    match field.kind {
        FieldKind::Text | FieldKind::Textarea => {
            let text = value
                .as_str()
                .ok_or_else(|| field_error(field, "KV4002", "必须为字符串"))?;
            if field.required && text.chars().all(char::is_whitespace) {
                return Err(field_error(field, "KV4001", "必填文本不能为空或全部为空白"));
            }
            if field
                .max_length
                .is_some_and(|limit| text.chars().count() > limit)
            {
                return Err(field_error(
                    field,
                    "KV4003",
                    "文本长度超过 maxLength（按 Unicode 标量计数）",
                ));
            }
            Ok(value.clone())
        }
        FieldKind::Number => {
            let number = value
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    field_error(field, "KV4002", "必须为有限 JSON 数字，不能使用数字字符串")
                })?;
            if field.integer && (number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER) {
                return Err(field_error(
                    field,
                    "KV4004",
                    "必须为 ±9007199254740991 范围内的安全整数",
                ));
            }
            if field.min.is_some_and(|min| number < min)
                || field.max.is_some_and(|max| number > max)
            {
                return Err(field_error(field, "KV4005", "数字不满足 min/max 包含边界"));
            }
            if field.integer {
                // 仅在有限、整数、安全范围检查后转换，不截断用户小数或溢出值。
                Ok(Value::Number(Number::from(number as i64)))
            } else {
                Number::from_f64(number)
                    .map(Value::Number)
                    .ok_or_else(|| field_error(field, "KV4002", "数字无法表示为有限 JSON 值"))
            }
        }
        FieldKind::Boolean => value.as_bool().map(Value::Bool).ok_or_else(|| {
            field_error(
                field,
                "KV4002",
                "必须为 true 或 false，不能使用 0、1 或字符串",
            )
        }),
        FieldKind::Date => {
            let text = value
                .as_str()
                .ok_or_else(|| field_error(field, "KV4002", "日期必须为字符串"))?;
            parse_date(text).map(|_| value.clone()).ok_or_else(|| {
                field_error(
                    field,
                    "KV4006",
                    "日期必须为 0001..9999 年内的有效 YYYY-MM-DD 日历日期",
                )
            })
        }
        FieldKind::Datetime => {
            let text = value
                .as_str()
                .ok_or_else(|| field_error(field, "KV4002", "日期时间必须为字符串"))?;
            normalize_datetime(text).map(Value::String).ok_or_else(|| {
                field_error(
                    field,
                    "KV4007",
                    "日期时间必须使用显式时区 RFC3339、最多 3 位小数秒；不接受闰秒或超范围年份",
                )
            })
        }
        FieldKind::Select => {
            let text = value
                .as_str()
                .ok_or_else(|| field_error(field, "KV4002", "选项值必须为字符串"))?;
            if field.required && text.chars().all(char::is_whitespace) {
                return Err(field_error(
                    field,
                    "KV4001",
                    "必填选项值不能为空或全部为空白",
                ));
            }
            if !field.options.iter().any(|option| option.value == text) {
                return Err(field_error(field, "KV4008", "值不属于声明的选项"));
            }
            Ok(value.clone())
        }
    }
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

fn parse_date(text: &str) -> Option<NaiveDate> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, byte)| i == 4 || i == 7 || byte.is_ascii_digit())
    {
        return None;
    }
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    (1..=9999).contains(&date.year()).then_some(date)
}

fn normalize_datetime(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || !text.is_ascii()
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || parse_date(&text[..10]).is_none()
    {
        return None;
    }
    let time = &bytes[11..19];
    if !time
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 2 || index == 5 || byte.is_ascii_digit())
    {
        return None;
    }
    let mut offset = 19;
    if bytes[offset] == b'.' {
        offset += 1;
        let fraction_start = offset;
        while bytes.get(offset).is_some_and(u8::is_ascii_digit) {
            offset += 1;
        }
        if !(1..=3).contains(&(offset - fraction_start)) {
            return None;
        }
    }
    let timezone = bytes.get(offset..)?;
    if timezone != b"Z"
        && !(timezone.len() == 6
            && matches!(timezone[0], b'+' | b'-')
            && timezone[3] == b':'
            && timezone[1..3]
                .iter()
                .chain(&timezone[4..6])
                .all(u8::is_ascii_digit))
    {
        return None;
    }
    let parsed = DateTime::parse_from_rfc3339(text).ok()?;
    if parsed.second() >= 60 || parsed.nanosecond() >= 1_000_000_000 {
        return None;
    }
    let utc = parsed.with_timezone(&Utc);
    if !(1..=9999).contains(&utc.year()) {
        return None;
    }
    Some(utc.to_rfc3339_opts(SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::SelectOption, dsl::SourceLocation};
    use serde_json::json;

    fn field(kind: FieldKind) -> FieldDefinition {
        FieldDefinition {
            name: "value".into(),
            label: "值".into(),
            kind,
            required: false,
            default: None,
            integer: false,
            min: None,
            max: None,
            max_length: None,
            unique: false,
            index: false,
            options: vec![],
            source: SourceLocation {
                path: "test.tsx".into(),
                span: kengine::SourceSpan {
                    start: 0,
                    end: 0,
                    start_position: kengine::Position { line: 1, column: 1 },
                    end_position: kengine::Position { line: 1, column: 1 },
                },
            },
        }
    }

    #[test]
    fn shared_vectors_cover_every_scalar_kind_and_canonical_values()
    -> Result<(), Box<dyn std::error::Error>> {
        let vectors: Value =
            serde_json::from_str(include_str!("../tests/fixtures/validation.json"))?;
        for case in vectors.as_array().ok_or("向量必须为数组")? {
            let kind: FieldKind = serde_json::from_value(case["kind"].clone())?;
            let mut definition = field(kind);
            let rules = &case["rules"];
            definition.required = rules["required"].as_bool().unwrap_or(false);
            definition.integer = rules["integer"].as_bool().unwrap_or(false);
            definition.min = rules["min"].as_f64();
            definition.max = rules["max"].as_f64();
            definition.max_length = rules["max_length"].as_u64().map(|value| value as usize);
            if let Some(options) = rules["options"].as_array() {
                definition.options = options
                    .iter()
                    .map(|value| SelectOption {
                        value: value.as_str().unwrap_or_default().into(),
                        label: String::new(),
                    })
                    .collect();
            }
            let result = validate_field(&definition, &case["input"]);
            assert_eq!(
                result.is_ok(),
                case["valid"].as_bool().ok_or("缺少 valid")?,
                "{}: {result:?}",
                case["name"]
            );
            if let Some(expected) = case.get("output") {
                assert_eq!(&result?, expected, "{}", case["name"]);
            }
        }
        Ok(())
    }

    #[test]
    fn create_defaults_patch_absence_null_and_unknown_keys_have_distinct_meaning()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut name = field(FieldKind::Text);
        name.name = "name".into();
        name.required = true;
        let mut enabled = field(FieldKind::Boolean);
        enabled.name = "enabled".into();
        enabled.default = Some(json!(false));
        let mut age = field(FieldKind::Number);
        age.name = "age".into();
        age.integer = true;
        let resource = ResourceDefinition {
            name: "records".into(),
            fields: vec![name, enabled, age],
            source: field(FieldKind::Text).source,
        };
        assert_eq!(
            validate_record(&resource, &json!({"name":" 张三 "}), WriteMode::Create)?,
            json!({"name":" 张三 ","enabled":false,"age":null})
                .as_object()
                .ok_or("必须为对象")?
                .clone()
        );
        assert!(validate_record(&resource, &json!({}), WriteMode::Create).is_err());
        assert!(validate_record(&resource, &json!({}), WriteMode::Patch)?.is_empty());
        assert_eq!(
            validate_record(&resource, &json!({"age":null}), WriteMode::Patch)?.get("age"),
            Some(&Value::Null)
        );
        assert!(validate_record(&resource, &json!({"name":null}), WriteMode::Patch).is_err());
        assert!(
            validate_record(&resource, &json!({"id":1,"name":"张三"}), WriteMode::Create).is_err()
        );
        assert!(validate_record(&resource, &json!([]), WriteMode::Patch).is_err());
        Ok(())
    }

    #[test]
    fn json_parser_rejects_duplicate_keys_trailing_input_and_nonobjects()
    -> Result<(), Box<dyn std::error::Error>> {
        for input in [
            br#"{"name":"a","name":"b"}"#.as_slice(),
            br#"{"name":1,"\u006eame":2}"#,
            br#"{"name":{"nested":1,"nested":2}}"#,
            br#"{"values":[{"a":1,"\u0061":2}]}"#,
            br#"{}{}"#,
            br#"[]"#,
            br#"null"#,
            br#"{"value":1e999}"#,
        ] {
            assert!(parse_record_json(input).is_err());
        }
        assert_eq!(parse_record_json(br#"{"name":"a"}"#)?, json!({"name":"a"}));
        assert_eq!(
            parse_record_json(br#"{"values":[null,true,1,2.5,{"a":"b"}]}"#)?,
            json!({"values":[null,true,1,2.5,{"a":"b"}]})
        );
        Ok(())
    }
}
