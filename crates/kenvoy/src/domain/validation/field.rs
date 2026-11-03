use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use serde_json::{Number, Value};

use crate::domain::model::{FieldDefinition, FieldKind};

use super::{
    errors::{ValidationError, field_error},
    files::validate_file_field,
};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// 验证并规范化单个显式值。可空性由 !required 决定；不做隐式字符串/布尔转换。
pub fn validate_field(field: &FieldDefinition, value: &Value) -> Result<Value, ValidationError> {
    if value.as_str().is_some_and(|text| text.contains('\0')) {
        return Err(field_error(
            field,
            "KV4002",
            "字段文本不能包含 NUL 字符",
            "validation.text.nul",
        ));
    }
    if value.is_null() {
        return if field.required {
            Err(field_error(
                field,
                "KV4001",
                "必填字段不能为 null",
                "validation.required",
            ))
        } else {
            Ok(Value::Null)
        };
    }
    match field.kind {
        FieldKind::Text | FieldKind::Textarea => {
            let text = value.as_str().ok_or_else(|| {
                field_error(field, "KV4002", "必须为字符串", "validation.type.string")
            })?;
            if field.required && text.chars().all(char::is_whitespace) {
                return Err(field_error(
                    field,
                    "KV4001",
                    "必填文本不能为空或全部为空白",
                    "validation.required.text",
                ));
            }
            if let Some(max) = field
                .max_length
                .filter(|limit| text.chars().count() > *limit)
            {
                return Err(field_error(
                    field,
                    "KV4003",
                    "文本长度超过 maxLength（按 Unicode 标量计数）",
                    "validation.text.max_length",
                )
                .with_param("max", max));
            }
            Ok(value.clone())
        }
        FieldKind::Number => {
            let number = value
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| {
                    field_error(
                        field,
                        "KV4002",
                        "必须为有限 JSON 数字，不能使用数字字符串",
                        "validation.type.number",
                    )
                })?;
            if field.integer && (number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER) {
                return Err(field_error(
                    field,
                    "KV4004",
                    "必须为 ±9007199254740991 范围内的安全整数",
                    "validation.number.integer",
                )
                .with_param("min", -(MAX_SAFE_INTEGER as i64))
                .with_param("max", MAX_SAFE_INTEGER as i64));
            }
            if let Some(min) = field.min.filter(|min| number < *min) {
                return Err(field_error(
                    field,
                    "KV4005",
                    "数字不满足 min/max 包含边界",
                    "validation.number.min",
                )
                .with_param("min", min));
            }
            if let Some(max) = field.max.filter(|max| number > *max) {
                return Err(field_error(
                    field,
                    "KV4005",
                    "数字不满足 min/max 包含边界",
                    "validation.number.max",
                )
                .with_param("max", max));
            }
            if field.integer {
                // 仅在有限、整数、安全范围检查后转换，不截断用户小数或溢出值。
                Ok(Value::Number(Number::from(number as i64)))
            } else {
                Number::from_f64(number).map(Value::Number).ok_or_else(|| {
                    field_error(
                        field,
                        "KV4002",
                        "数字无法表示为有限 JSON 值",
                        "validation.type.number",
                    )
                })
            }
        }
        FieldKind::Boolean => value.as_bool().map(Value::Bool).ok_or_else(|| {
            field_error(
                field,
                "KV4002",
                "必须为 true 或 false，不能使用 0、1 或字符串",
                "validation.type.boolean",
            )
        }),
        FieldKind::Date => {
            let text = value.as_str().ok_or_else(|| {
                field_error(field, "KV4002", "日期必须为字符串", "validation.type.date")
            })?;
            parse_date(text).map(|_| value.clone()).ok_or_else(|| {
                field_error(
                    field,
                    "KV4006",
                    "日期必须为 0001..9999 年内的有效 YYYY-MM-DD 日历日期",
                    "validation.date",
                )
            })
        }
        FieldKind::Datetime => {
            let text = value.as_str().ok_or_else(|| {
                field_error(
                    field,
                    "KV4002",
                    "日期时间必须为字符串",
                    "validation.type.datetime",
                )
            })?;
            normalize_datetime(text).map(Value::String).ok_or_else(|| {
                field_error(
                    field,
                    "KV4007",
                    "日期时间必须使用显式时区 RFC3339、最多 3 位小数秒；不接受闰秒或超范围年份",
                    "validation.datetime",
                )
                .with_param("fraction_digits", 3)
            })
        }
        FieldKind::Select => {
            let text = value.as_str().ok_or_else(|| {
                field_error(
                    field,
                    "KV4002",
                    "选项值必须为字符串",
                    "validation.type.select",
                )
            })?;
            if field.required && text.chars().all(char::is_whitespace) {
                return Err(field_error(
                    field,
                    "KV4001",
                    "必填选项值不能为空或全部为空白",
                    "validation.required.select",
                ));
            }
            if field.options_source.is_none()
                && !field.options.iter().any(|option| option.value == text)
            {
                return Err(field_error(
                    field,
                    "KV4008",
                    "值不属于声明的选项",
                    "validation.enum",
                ));
            }
            Ok(value.clone())
        }
        FieldKind::File => validate_file_field(field, value),
    }
}

pub fn validate_default(field: &FieldDefinition, value: &Value) -> Result<Value, ValidationError> {
    if field.kind == FieldKind::File
        && !value.is_null()
        && !value.as_array().is_some_and(Vec::is_empty)
    {
        return Err(field_error(
            field,
            "KV4014",
            "file 默认值只允许 null 或 []，不能预绑定附件",
            "validation.files.default",
        ));
    }
    validate_field(field, value)
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
