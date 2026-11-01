//! 与传输层无关的字段约束；DSL 默认值、资源写入和浏览器规则共享同一契约。

use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Timelike, Utc};
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Number, Value};

use crate::application::{
    FieldDefinition, FieldKind, FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE,
    ResourceDefinition, UploadedFile,
};

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

fn mime_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$%&'+-.^_`|~".contains(&byte)
        })
}

pub fn valid_mime_type(value: &str) -> bool {
    value
        .split_once('/')
        .is_some_and(|(kind, subtype)| mime_token(kind) && mime_token(subtype))
}

pub fn mime_matches_accept(content_type: &str, accept: &[String]) -> bool {
    accept.is_empty()
        || accept.iter().any(|allowed| {
            allowed == content_type
                || allowed.strip_suffix("/*").is_some_and(|kind| {
                    content_type
                        .split_once('/')
                        .is_some_and(|(actual, _)| actual == kind)
                })
        })
}

pub fn validate_file_constraints(constraints: &FileConstraints) -> Result<(), &'static str> {
    if !(1..=MAX_UPLOAD_SIZE).contains(&constraints.max_size) {
        return Err("maxSize 必须为 1 到 10485760 的整数");
    }
    if !(1..=MAX_UPLOAD_FILES).contains(&constraints.max_files) {
        return Err("maxFiles 必须为 1 到 20 的整数");
    }
    let mut seen = BTreeSet::new();
    for accepted in &constraints.accept {
        if !(valid_mime_type(accepted) || accepted.strip_suffix("/*").is_some_and(mime_token)) {
            return Err("accept 只允许规范小写 MIME 或 type/*，不接受后缀和 MIME 参数");
        }
        if !seen.insert(accepted) {
            return Err("accept 不能包含重复 MIME");
        }
    }
    Ok(())
}

pub fn valid_upload_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_file_metadata(
    id: &str,
    name: &str,
    content_type: &str,
    size: u64,
) -> Result<(), &'static str> {
    if !valid_upload_id(id) {
        return Err("文件 id 必须为 64 位小写十六进制");
    }
    if name.is_empty()
        || name.chars().count() > 255
        || matches!(name, "." | "..")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err("文件名必须为不超过 255 个 Unicode 标量的非空名称，不能包含路径或控制字符");
    }
    if !(1..=MAX_UPLOAD_SIZE).contains(&size) {
        return Err("文件大小必须为 1 到 10485760 字节");
    }
    if !valid_mime_type(content_type) {
        return Err("文件 content_type 必须为不含参数的规范小写 MIME");
    }
    Ok(())
}

pub fn validate_uploaded_file(file: &UploadedFile) -> Result<(), &'static str> {
    validate_file_metadata(&file.id, &file.name, &file.content_type, file.size)
}

/// 图片声明和客户端投影只接收明确的 HTTP(S) URL 或站内绝对路径。
pub fn valid_image_src(src: &str) -> bool {
    if src.is_empty()
        || src.trim() != src
        || src.contains('\\')
        || src.chars().any(char::is_control)
    {
        return false;
    }
    if src.starts_with('/') {
        return !src.starts_with("//");
    }
    let Some((scheme, remainder)) = src.split_once("://") else {
        return false;
    };
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || remainder
            .split(['/', '?', '#'])
            .next()
            .is_none_or(|authority| authority.is_empty() || authority.contains('@'))
    {
        return false;
    }
    let Ok(url) = url::Url::parse(src) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
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
            options_source: None,
            upload: (kind == FieldKind::File).then(FileConstraints::default),
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

    fn uploaded(id: char) -> Value {
        json!({
            "id": id.to_string().repeat(64),
            "name": "图片.png",
            "content_type": "image/png",
            "size": 2
        })
    }

    #[test]
    fn files_validate_strict_metadata_arrays_and_real_constraint_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut definition = field(FieldKind::File);
        definition.upload = Some(FileConstraints {
            accept: vec!["image/*".into()],
            max_size: MAX_UPLOAD_SIZE,
            max_files: 2,
        });
        let files = json!([uploaded('a'), uploaded('b')]);
        assert_eq!(validate_field(&definition, &files)?, files);
        assert_eq!(validate_field(&definition, &json!([]))?, json!([]));
        assert_eq!(validate_field(&definition, &Value::Null)?, Value::Null);
        for invalid in [
            json!({}),
            json!("[]"),
            json!([null]),
            json!([uploaded('a'), uploaded('a')]),
            json!([uploaded('a'), uploaded('b'), uploaded('c')]),
        ] {
            assert!(validate_field(&definition, &invalid).is_err(), "{invalid}");
        }
        for (key, value) in [
            ("id", json!("a".repeat(63))),
            ("id", json!("A".repeat(64))),
            ("name", json!("")),
            ("name", json!(".")),
            ("name", json!("..")),
            ("name", json!("a/b")),
            ("name", json!("a\\b")),
            ("name", json!("a\u{0000}b")),
            ("name", json!("图".repeat(256))),
            ("content_type", json!("Image/png")),
            ("content_type", json!("image/*")),
            ("content_type", json!("image/png; charset=utf-8")),
            ("content_type", json!("application/pdf")),
            ("size", json!(0)),
            ("size", json!(-1)),
            ("size", json!(1.5)),
            ("size", json!(MAX_UPLOAD_SIZE + 1)),
        ] {
            let mut file = uploaded('a');
            file[key] = value;
            assert!(
                validate_field(&definition, &json!([file])).is_err(),
                "{key}"
            );
        }
        let mut extra = uploaded('a');
        extra["url"] = json!("/untrusted");
        assert!(validate_field(&definition, &json!([extra.clone()])).is_err());
        assert!(serde_json::from_value::<UploadedFile>(extra).is_err());
        let mut missing = uploaded('a');
        missing
            .as_object_mut()
            .ok_or("缺少附件对象")?
            .remove("name");
        assert!(validate_field(&definition, &json!([missing])).is_err());
        let mut boundary = uploaded('a');
        boundary["name"] = json!("图".repeat(255));
        boundary["size"] = json!(MAX_UPLOAD_SIZE);
        assert!(validate_field(&definition, &json!([boundary])).is_ok());
        definition.upload.as_mut().ok_or("缺少上传约束")?.max_size = 1;
        assert!(validate_field(&definition, &json!([uploaded('a')])).is_err());
        definition.required = true;
        for invalid in [Value::Null, json!([])] {
            assert_eq!(
                validate_field(&definition, &invalid)
                    .expect_err("必填附件不能为空")
                    .code,
                "KV4001"
            );
        }
        for kind in [
            FieldKind::Text,
            FieldKind::Textarea,
            FieldKind::Number,
            FieldKind::Boolean,
            FieldKind::Date,
            FieldKind::Datetime,
            FieldKind::Select,
        ] {
            assert!(validate_field(&field(kind), &files).is_err(), "{kind:?}");
        }
        Ok(())
    }

    #[test]
    fn file_defaults_cannot_embed_references_and_mime_constraints_are_canonical() {
        let mut definition = field(FieldKind::File);
        assert!(validate_default(&definition, &json!([])).is_ok());
        assert!(validate_default(&definition, &Value::Null).is_ok());
        assert!(validate_default(&definition, &json!([uploaded('a')])).is_err());
        definition.required = true;
        assert!(validate_default(&definition, &json!([])).is_err());
        assert!(validate_default(&definition, &Value::Null).is_err());
        for constraints in [
            FileConstraints {
                max_size: 0,
                ..FileConstraints::default()
            },
            FileConstraints {
                max_size: MAX_UPLOAD_SIZE + 1,
                ..FileConstraints::default()
            },
            FileConstraints {
                max_files: 0,
                ..FileConstraints::default()
            },
            FileConstraints {
                max_files: MAX_UPLOAD_FILES + 1,
                ..FileConstraints::default()
            },
            FileConstraints {
                accept: vec![".png".into()],
                ..FileConstraints::default()
            },
            FileConstraints {
                accept: vec!["IMAGE/png".into()],
                ..FileConstraints::default()
            },
            FileConstraints {
                accept: vec!["*/*".into()],
                ..FileConstraints::default()
            },
            FileConstraints {
                accept: vec!["image/png".into(), "image/png".into()],
                ..FileConstraints::default()
            },
        ] {
            assert!(
                validate_file_constraints(&constraints).is_err(),
                "{constraints:?}"
            );
        }
        assert!(valid_mime_type("application/vnd.example+json"));
        assert!(mime_matches_accept("image/webp", &["image/*".into()]));
        assert!(!mime_matches_accept("application/pdf", &["image/*".into()]));
    }

    #[test]
    fn image_sources_reject_unsafe_schemes_credentials_and_url_repair() {
        for src in [
            "/images/图片.png",
            "/_kenvoy/files/abc?download=1",
            "https://example.com/image.png",
            "HTTP://example.com/image.png",
            "http://[::1]:8080/image.png",
        ] {
            assert!(valid_image_src(src), "{src}");
        }
        for src in [
            "",
            "relative.png",
            "//example.com/a.png",
            "/\\example.com/a.png",
            "https://example.com\\a.png",
            "https://example.com/\nimage.png",
            " https://example.com/a.png",
            "https://user:password@example.com/a.png",
            "https://@example.com/a.png",
            "https:example.com/a.png",
            "https:////example.com/a.png",
            "javascript:alert(1)",
            "data:image/png;base64,AA",
            "blob:https://example.com/image",
        ] {
            assert!(!valid_image_src(src), "{src}");
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
                        when: Default::default(),
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
