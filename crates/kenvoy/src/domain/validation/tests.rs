use super::{
    WriteMode, mime_matches_accept, parse_record_json, valid_image_src, valid_mime_type,
    validate_default, validate_field, validate_file_constraints, validate_record,
};
use crate::domain::{
    model::{
        FieldDefinition, FieldKind, FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE,
        ResourceDefinition, SelectOption, UploadedFile,
    },
    source::SourceLocation,
};
use serde_json::{Value, json};

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
        serde_json::from_str(include_str!("../../../tests/fixtures/validation.json"))?;
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
    assert!(validate_record(&resource, &json!({"id":1,"name":"张三"}), WriteMode::Create).is_err());
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

#[test]
fn rule_metadata_reports_real_bounds_without_embedding_input_values()
-> Result<(), Box<dyn std::error::Error>> {
    let mut number = field(FieldKind::Number);
    number.min = Some(3.0);
    number.max = Some(9.0);
    for (input, key, params) in [
        (json!(2), "validation.number.min", json!({"min": 3.0})),
        (json!(10), "validation.number.max", json!({"max": 9.0})),
    ] {
        let error = validate_field(&number, &input).expect_err("越界必须拒绝");
        assert_eq!(error.code, "KV4005");
        assert_eq!(error.field.as_deref(), Some("value"));
        assert_eq!(error.message_key, key);
        assert_eq!(serde_json::to_value(error.params)?, params);
    }
    number.integer = true;
    let error = validate_field(&number, &json!(3.5)).expect_err("小数不能作为整数");
    assert_eq!(error.message_key, "validation.number.integer");
    let parameters: Value = serde_json::from_slice(&serde_json::to_vec(&error.params)?)?;
    assert_eq!(parameters["min"].as_i64(), Some(-9_007_199_254_740_991));
    assert_eq!(parameters["max"].as_i64(), Some(9_007_199_254_740_991));
    let mut text = field(FieldKind::Text);
    text.max_length = Some(2);
    assert_eq!(validate_field(&text, &json!("😀好"))?, json!("😀好"));
    let error = validate_field(&text, &json!("😀你好")).expect_err("Unicode 标量超限");
    assert_eq!(error.message_key, "validation.text.max_length");
    assert_eq!(serde_json::to_value(error.params)?, json!({"max": 2}));
    let datetime = field(FieldKind::Datetime);
    let error = validate_field(&datetime, &json!("2026-01-01T10:00:00")).expect_err("缺少显式时区");
    assert_eq!(error.message_key, "validation.datetime");
    assert_eq!(
        serde_json::to_value(error.params)?,
        json!({"fraction_digits": 3})
    );
    assert_eq!(
        validate_field(&datetime, &json!("2026-01-01T10:00:00+08:00"))?,
        json!("2026-01-01T02:00:00.000Z")
    );
    Ok(())
}

#[test]
fn required_and_option_errors_have_distinct_machine_rules() {
    for (kind, input, key) in [
        (FieldKind::Text, Value::Null, "validation.required"),
        (FieldKind::Text, json!(" \t"), "validation.required.text"),
        (FieldKind::Select, json!(" "), "validation.required.select"),
        (FieldKind::File, json!([]), "validation.required.files"),
    ] {
        let mut definition = field(kind);
        definition.required = true;
        let error = validate_field(&definition, &input).expect_err("必填值必须拒绝");
        assert_eq!(error.code, "KV4001");
        assert_eq!(error.message_key, key);
        assert!(error.params.is_empty());
        assert!(!error.message.is_empty());
    }
    let error = validate_field(&field(FieldKind::Select), &json!("private-value"))
        .expect_err("未声明选项必须拒绝");
    assert_eq!(error.message_key, "validation.enum");
    assert!(error.params.is_empty());
    let error =
        parse_record_json(br#"{"secret":1,"secret":2}"#).expect_err("重复 JSON 字段必须拒绝");
    assert_eq!(error.message_key, "validation.record.json");
    assert!(error.params.is_empty());
}

#[test]
fn attachment_metadata_exposes_only_actual_limits() -> Result<(), Box<dyn std::error::Error>> {
    let mut definition = field(FieldKind::File);
    definition.upload = Some(FileConstraints {
        accept: vec!["image/*".into()],
        max_files: 1,
        max_size: 1,
    });
    for (input, key, params) in [
        (
            json!([uploaded('a'), uploaded('b')]),
            "validation.files.max_files",
            json!({"max": 1}),
        ),
        (
            json!([uploaded('a')]),
            "validation.files.max_size",
            json!({"max": 1}),
        ),
    ] {
        let error = validate_field(&definition, &input).expect_err("附件越界必须拒绝");
        assert_eq!(error.code, "KV4014");
        assert_eq!(error.message_key, key);
        assert_eq!(serde_json::to_value(error.params)?, params);
    }
    let mut invalid = uploaded('a');
    invalid["id"] = json!("invalid-id");
    let error = validate_field(&definition, &json!([invalid])).expect_err("附件标识必须拒绝");
    assert_eq!(error.message_key, "validation.files.id");
    assert_eq!(serde_json::to_value(error.params)?, json!({"length": 64}));
    let constraints = FileConstraints {
        max_size: 0,
        ..FileConstraints::default()
    };
    let error = validate_file_constraints(&constraints)
        .expect_err("上传约束必须拒绝")
        .into_validation("value");
    assert_eq!(error.message_key, "validation.files.constraints.max_size");
    assert_eq!(
        serde_json::to_value(error.params)?,
        json!({"min": 1, "max": MAX_UPLOAD_SIZE})
    );
    Ok(())
}
