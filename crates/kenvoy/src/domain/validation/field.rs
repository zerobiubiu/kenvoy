use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use serde_json::{Number, Value};

use crate::domain::model::{FieldDefinition, FieldKind};

use super::{
    errors::{ValidationError, field_error},
    files::{mime_matches_accept, validate_file_constraints, validate_file_metadata},
};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

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
            if field.options_source.is_none()
                && !field.options.iter().any(|option| option.value == text)
            {
                return Err(field_error(field, "KV4008", "值不属于声明的选项"));
            }
            Ok(value.clone())
        }
        FieldKind::File => {
            let constraints = field
                .upload
                .as_ref()
                .ok_or_else(|| field_error(field, "KV4014", "file 字段缺少上传约束"))?;
            validate_file_constraints(constraints)
                .map_err(|message| field_error(field, "KV4014", message))?;
            let files = value
                .as_array()
                .ok_or_else(|| field_error(field, "KV4002", "文件字段必须为附件元数据数组"))?;
            if field.required && files.is_empty() {
                return Err(field_error(field, "KV4001", "必填附件至少需要一个文件"));
            }
            if files.len() > constraints.max_files {
                return Err(field_error(field, "KV4014", "附件数量超过 maxFiles"));
            }
            let mut ids = BTreeSet::new();
            for file in files {
                let object = file
                    .as_object()
                    .filter(|object| {
                        object.len() == 4
                            && ["id", "name", "content_type", "size"]
                                .iter()
                                .all(|key| object.contains_key(*key))
                    })
                    .ok_or_else(|| {
                        field_error(
                            field,
                            "KV4014",
                            "附件必须且只能包含 id/name/content_type/size",
                        )
                    })?;
                let metadata = object["id"]
                    .as_str()
                    .zip(object["name"].as_str())
                    .zip(object["content_type"].as_str())
                    .zip(object["size"].as_u64())
                    .ok_or_else(|| {
                        field_error(
                            field,
                            "KV4014",
                            "附件 id/name/content_type 必须为字符串，size 必须为正整数",
                        )
                    })?;
                let (((id, name), content_type), size) = metadata;
                validate_file_metadata(id, name, content_type, size)
                    .map_err(|message| field_error(field, "KV4014", message))?;
                if !ids.insert(id) {
                    return Err(field_error(
                        field,
                        "KV4014",
                        "附件数组不能重复引用同一个文件",
                    ));
                }
                if size > constraints.max_size {
                    return Err(field_error(field, "KV4014", "附件大小超过 maxSize"));
                }
                if !mime_matches_accept(content_type, &constraints.accept) {
                    return Err(field_error(field, "KV4014", "附件 MIME 不满足 accept"));
                }
            }
            Ok(value.clone())
        }
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
