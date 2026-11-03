//! 集中消息矩阵由真实 Schema 引擎和 CLI 共同约束，不按 DSL 固定词条。

use std::fs;

use serde_json::{Value, json};

use super::{Fixture, Result, reject, snapshot, success};

#[test]
fn generated_schema_accepts_metadata_and_complete_code_or_alias_rows() -> Result {
    let fixture = Fixture::new()?;
    success(&fixture.command(&["i18n"])?)?;
    let schema: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("language.schema.json"))?)?;
    let validator = jsonschema::validator_for(&schema)?;
    for valid in [
        json!({"$schema":"./language.schema.json"}),
        json!({"$schema":"./language.schema.json","arbitrary":{"zh-CN":"中","en":"English","ko-KR":"한국어"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"标题","en-US":"Title","ko":"제목"},"content":{"zh-CN":"内容","en":"Content","ko-KR":"내용"}}),
    ] {
        assert!(validator.is_valid(&valid), "应接受完整矩阵：{valid}");
        fs::write(
            fixture.project().join("translations.json"),
            serde_json::to_vec(&valid)?,
        )?;
        let before = snapshot(&fixture.project())?;
        success(&fixture.command(&["check"])?)?;
        success(&fixture.command(&["i18n"])?)?;
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    for invalid in [
        json!({}),
        json!({"$schema":"./other.schema.json"}),
        json!({"$schema":null}),
        json!({"$schema":"./language.schema.json","title":{}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"中","en":"English"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"中","zh-CN":"另一值","en":"English","ko":"한국어"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"中","en":"English","en-US":"Second","ko":"한국어"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"中","en":"English","ko":"한국어","ko-KR":"다른 값"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"中","en":"English","ko":"한국어","fr":"Français"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":"","en":"English","ko":"한국어"}}),
        json!({"$schema":"./language.schema.json","title":{"cn":3,"en":"English","ko":"한국어"}}),
    ] {
        assert!(
            !validator.is_valid(&invalid),
            "Schema 不得放过非法矩阵：{invalid}"
        );
    }
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn malformed_incomplete_or_duplicate_messages_fail_read_only_and_never_get_overwritten() -> Result {
    let fixture = Fixture::new()?;
    success(&fixture.command(&["i18n"])?)?;
    let path = fixture.project().join("translations.json");
    for invalid in [
        r#"{}"#,
        r#"{"$schema":"https://example.invalid/language.schema.json"}"#,
        r#"{"$schema":"language.schema.json"}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","zh-CN":"另值","en":"English","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English","en-US":"Second","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English","ko":"한국어","ko-KR":"다른 값"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English","ko":"한국어","ja":"日本語"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"","en":"English","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":null,"en":"English","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":{"value":"中"},"en":"English","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":"not a language map"}"#,
        r#"{"$schema":"./language.schema.json","title":{}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English","ko":"한국어"},"title":{"cn":"再","en":"Again","ko":"다시"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"First","en":"Second","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"First","\u0065n":"Second","ko":"한국어"}}"#,
        r#"{"$schema":"./language.schema.json","$schema":"./language.schema.json"}"#,
        r#"{"$schema":"./language.schema.json","\u0024schema":"./language.schema.json"}"#,
        r#"["not a message map"]"#,
        r#"{"$schema":"./language.schema.json","title":"unterminated"#,
    ] {
        fs::write(&path, invalid)?;
        let before = snapshot(&fixture.project())?;
        for args in [&["check"][..], &["build"][..], &["i18n"][..]] {
            let diagnostic = reject(&fixture.command(args)?)?;
            assert!(diagnostic.contains("translations.json"), "{diagnostic}");
            assert_eq!(snapshot(&fixture.project())?, before, "{invalid}");
            assert_eq!(fs::read_to_string(&path)?, invalid);
            assert!(!fixture.project().join("data/app.sqlite").exists());
            assert!(!fixture.project().join("dist").exists());
        }
    }
    fs::write(&path, r#"{"$schema":"./language.schema.json"}"#)?;
    success(&fixture.command(&["i18n"])?)?;
    success(&fixture.command(&["check"])?)?;
    Ok(())
}

#[test]
fn missing_translations_cannot_be_repaired_by_read_only_commands() -> Result {
    let fixture = Fixture::new()?;
    let path = fixture.project().join("translations.json");
    fs::remove_file(&path)?;
    let before = snapshot(&fixture.project())?;
    for args in [&["check"][..], &["build"][..]] {
        let diagnostic = reject(&fixture.command(args)?)?;
        assert!(diagnostic.contains("kenvoy i18n"), "{diagnostic}");
        assert_eq!(snapshot(&fixture.project())?, before);
        assert!(!path.exists());
    }
    success(&fixture.command(&["i18n"])?)?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path)?)?,
        json!({"$schema":"./language.schema.json"})
    );
    success(&fixture.command(&["check"])?)?;
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn generated_schema_is_not_an_editable_validation_authority_or_a_read_only_output() -> Result {
    let fixture = Fixture::new()?;
    let path = fixture.project().join("language.schema.json");
    fs::write(&path, r#"{"not":{}}"#)?;
    let before = snapshot(&fixture.project())?;
    success(&fixture.command(&["check"])?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    fs::remove_file(&path)?;
    let before = snapshot(&fixture.project())?;
    success(&fixture.command(&["check"])?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    assert!(!path.exists());
    success(&fixture.command(&["i18n"])?)?;
    assert!(path.is_file());
    Ok(())
}

#[test]
fn schema_or_translations_directory_is_never_replaced_by_i18n() -> Result {
    let fixture = Fixture::new()?;
    for name in ["language.schema.json", "translations.json"] {
        let path = fixture.project().join(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::create_dir(&path)?;
        let marker = path.join("owned-marker");
        fs::write(&marker, b"must survive")?;
        reject(&fixture.command(&["i18n"])?)?;
        assert!(path.is_dir());
        assert_eq!(fs::read(&marker)?, b"must survive");
        fs::remove_file(marker)?;
        fs::remove_dir(path)?;
        success(&fixture.command(&["i18n"])?)?;
    }
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
