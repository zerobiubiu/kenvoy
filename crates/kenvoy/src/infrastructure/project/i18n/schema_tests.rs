use serde_json::json;

use crate::domain::i18n::LanguagesConfig;

use super::schema;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn generated_schema_is_valid_draft_2020_12_with_a_metadata_exception() -> TestResult {
    let generated = schema(&LanguagesConfig::default())?;
    assert!(jsonschema::meta::is_valid(&generated));
    assert_eq!(
        generated["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(generated["required"], json!(["$schema"]));
    assert_eq!(generated["minProperties"], 1);
    assert_eq!(
        generated["properties"]["$schema"]["const"],
        "./language.schema.json"
    );
    assert_eq!(generated["additionalProperties"]["required"], json!([]));
    let validator = jsonschema::validator_for(&generated)?;
    for valid in [
        json!({"$schema": "./language.schema.json"}),
        json!({"$schema": "./language.schema.json", "custom": {
            "zh-CN": "中文", "en": "English", "ko-KR": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "": {
            "cn": " ", "en-US": "English", "ko": "한국어"
        }}),
    ] {
        assert!(validator.is_valid(&valid), "{valid}");
    }
    Ok(())
}

#[test]
fn engine_enforces_one_of_language_coverage_unknown_properties_and_nonempty_strings() -> TestResult
{
    let generated = schema(&LanguagesConfig::default())?;
    let validator = jsonschema::draft202012::options().build(&generated)?;
    for invalid in [
        json!({}),
        json!({"$schema": "https://example.invalid/schema"}),
        json!({"$schema": null}),
        json!({"$schema": "./language.schema.json", "title": {
            "zh-CN": "中文", "cn": "中文", "en": "English", "ko": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "cn": "中文", "en": "English"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "cn": "中文", "en": "English", "ko": "한국어", "fr": "Français"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "cn": "", "en": "English", "ko": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "cn": 1, "en": "English", "ko": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "cn": null, "en": "English", "ko": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "title": {
            "ZH-CN": "中文", "en": "English", "ko": "한국어"
        }}),
        json!({"$schema": "./language.schema.json", "title": "not an object"}),
        json!({"$schema": "./language.schema.json", "title": []}),
        json!({"$schema": "./language.schema.json", "title": {}}),
        json!([]),
    ] {
        assert!(!validator.is_valid(&invalid), "{invalid}");
    }
    Ok(())
}

#[test]
fn languages_without_alias_require_the_exact_declared_code() -> TestResult {
    let config: LanguagesConfig = toml::from_str(
        "fallback='en-us'\n[en-us]\ndisplay='English'\n[zh-CN]\nalias='cn'\ndisplay='中文'",
    )?;
    let generated = schema(&config)?;
    assert!(jsonschema::meta::is_valid(&generated));
    assert_eq!(
        generated["additionalProperties"]["allOf"],
        json!([
            {"required": ["en-us"]},
            {"oneOf": [{"required": ["zh-CN"]}, {"required": ["cn"]}]}
        ])
    );
    let validator = jsonschema::validator_for(&generated)?;
    assert!(validator.is_valid(&json!({
        "$schema": "./language.schema.json", "message": {"en-us": "Text", "cn": "文案"}
    })));
    assert!(!validator.is_valid(&json!({
        "$schema": "./language.schema.json", "message": {"en-US": "Text", "cn": "文案"}
    })));
    let mut invalid = config;
    invalid.definitions.get_mut("en-us").unwrap().alias = Some("CN".into());
    assert!(schema(&invalid).is_err());
    Ok(())
}
