use std::collections::BTreeMap;

use super::{I18nBundle, LanguagesConfig, config::LanguageDefinition};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn single(code: &str) -> LanguagesConfig {
    LanguagesConfig {
        fallback: code.into(),
        definitions: BTreeMap::from([(
            code.into(),
            LanguageDefinition {
                alias: None,
                display: "测试语言".into(),
            },
        )]),
    }
}

#[test]
fn defaults_are_a_valid_mandatory_three_language_bundle() -> TestResult {
    let config = LanguagesConfig::default();
    config.validate()?;
    assert_eq!(config.fallback, "zh-CN");
    for (code, alias, display) in [
        ("zh-CN", "cn", "简体中文"),
        ("en-US", "en", "English"),
        ("ko-KR", "ko", "한국어"),
    ] {
        assert_eq!(config.definitions[code].alias.as_deref(), Some(alias));
        assert_eq!(config.definitions[code].display, display);
    }
    let bundle = I18nBundle::default();
    bundle.validate()?;
    assert_eq!(bundle.format_version, 2);
    assert_eq!(bundle.fallback, "zh-CN");
    assert_eq!(bundle.languages.len(), 3);
    assert_eq!(bundle.messages.len(), 3);
    assert!(bundle.messages.values().all(BTreeMap::is_empty));
    Ok(())
}

#[test]
fn explicit_configuration_has_no_implicit_languages_aliases_or_displays() -> TestResult {
    let config: LanguagesConfig = toml::from_str("[zh-CN]\ndisplay='自定义中文'\n")?;
    config.validate()?;
    assert_eq!(config.fallback, "zh-CN");
    assert_eq!(config.definitions.len(), 1);
    assert_eq!(config.definitions["zh-CN"].alias, None);
    for source in [
        "",
        "fallback='zh-CN'",
        "[en-US]\ndisplay='English'",
        "[zh-CN]\ndisplay=''",
        "[zh-CN]\ndisplay='  '",
        "[zh-CN]\ndisplay='中文'\nalias=' '",
    ] {
        let invalid: LanguagesConfig = toml::from_str(source)?;
        assert!(invalid.validate().is_err(), "{source}");
    }
    for source in [
        "[zh-CN]",
        "[zh-CN]\ndisplay='中文'\nalias=['cn']",
        "[zh-CN]\ndisplay='中文'\nname='中文'",
        "[zh-CN]\ndisplay=1",
    ] {
        assert!(
            toml::from_str::<LanguagesConfig>(source).is_err(),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn aliases_and_codes_share_one_case_insensitive_namespace() {
    for (code, alias) in [
        ("zh-CN", "ZH-cn"),
        ("zh-CN", "en-US"),
        ("zh-CN", "EN"),
        ("en-US", "CN"),
        ("en-US", ""),
        ("en-US", " \t"),
    ] {
        let mut config = LanguagesConfig::default();
        config.definitions.get_mut(code).unwrap().alias = Some(alias.into());
        assert!(config.validate().is_err(), "{code}: {alias:?}");
    }
    let mut config = LanguagesConfig::default();
    config.definitions.get_mut("zh-CN").unwrap().alias = Some("Ä".into());
    config.definitions.get_mut("en-US").unwrap().alias = Some("ä".into());
    assert!(config.validate().is_err());
    config = LanguagesConfig::default();
    config
        .definitions
        .insert("zh-cn".into(), config.definitions["zh-CN"].clone());
    assert!(config.validate().is_err());
    for fallback in ["cn", "ZH-cn", "fr-FR", ""] {
        let config = LanguagesConfig {
            fallback: fallback.into(),
            ..Default::default()
        };
        assert!(config.validate().is_err(), "{fallback}");
    }
}

#[test]
fn bundle_rejects_old_fields_unknown_versions_and_incomplete_message_matrices() -> TestResult {
    let mut bundle = I18nBundle {
        format_version: 1,
        ..Default::default()
    };
    assert!(bundle.validate().is_err());
    bundle.format_version = 2;
    bundle.messages.remove("ko-KR");
    assert!(bundle.validate().is_err());
    bundle.messages.insert("ko-KR".into(), BTreeMap::new());
    bundle.messages.insert("fr-FR".into(), BTreeMap::new());
    assert!(bundle.validate().is_err());
    bundle.messages.remove("fr-FR");
    bundle
        .messages
        .get_mut("zh-CN")
        .unwrap()
        .insert("title".into(), "标题".into());
    assert!(bundle.validate().is_err());
    for catalog in bundle.messages.values_mut() {
        catalog.insert("title".into(), "显示文字".into());
    }
    bundle.validate()?;
    bundle
        .messages
        .get_mut("ko-KR")
        .unwrap()
        .insert("title".into(), String::new());
    assert!(bundle.validate().is_err());
    // 权限投影可在所有语言同步删除同一词条。
    for catalog in bundle.messages.values_mut() {
        catalog.remove("title");
    }
    bundle.validate()?;
    let mut encoded = serde_json::to_value(&bundle)?;
    encoded["default_locale"] = "zh-CN".into();
    assert!(serde_json::from_value::<I18nBundle>(encoded).is_err());
    Ok(())
}

#[test]
fn bundle_language_options_are_strict_and_alias_absence_is_serialized_as_absence() -> TestResult {
    let mut bundle = I18nBundle::default();
    bundle.languages[0].alias = None;
    let encoded = serde_json::to_value(&bundle)?;
    assert!(encoded["languages"][0].get("alias").is_none());
    bundle.languages[0].display = " ".into();
    assert!(bundle.validate().is_err());
    bundle = I18nBundle::default();
    bundle.languages.push(bundle.languages[0].clone());
    assert!(bundle.validate().is_err());
    let mut encoded = serde_json::to_value(I18nBundle::default())?;
    encoded["languages"][0]["extra"] = true.into();
    assert!(serde_json::from_value::<I18nBundle>(encoded).is_err());
    Ok(())
}

#[test]
fn bundle_decoding_rejects_duplicate_languages_messages_and_non_string_values() {
    for messages in [
        r#"{"zh-CN":{},"zh-CN":{}}"#,
        r#"{"zh-CN":{"label":"甲","label":"乙"}}"#,
        r#"{"zh-CN":{"label":"甲","\u006cabel":"乙"}}"#,
        r#"{"zh-CN":{"label":{"nested":"甲"}}}"#,
        r#"{"zh-CN":{"label":1}}"#,
    ] {
        let encoded = format!(
            r#"{{"format_version":2,"fallback":"zh-CN","languages":[{{"code":"zh-CN","display":"中文"}}],"messages":{messages}}}"#
        );
        assert!(
            serde_json::from_str::<I18nBundle>(&encoded).is_err(),
            "{messages}"
        );
    }
}

#[test]
fn language_codes_keep_the_existing_unicode_bcp47_structure_rules() {
    for code in [
        "zh-CN",
        "en-US",
        "fr",
        "zh-Hant-TW",
        "es-419",
        "sr-Latn-RS",
        "de-DE-1996",
        "sl-rozaj-biske",
        "en-US-u-ca-gregory",
        "en-x-project",
        "en-u-foo-ca-gregory",
        "en-t-zh-hant",
        "hi-Latn-t-en-h0-hybrid",
    ] {
        assert!(single(code).validate().is_ok(), "{code}");
    }
    for code in [
        "",
        "en_US",
        "../en",
        "en/US",
        " en-US",
        "en-US ",
        "中文",
        "e-US",
        "abcd",
        "en--US",
        "en-u",
        "en-x",
        "en-1234-1234",
        "en-u-ca-u-nu",
        "en-US-verylongtag",
        "en-@private",
        "x-private",
        "en-u-12",
        "en-u-ca-gregory-ca-buddhist",
        "en-t-en-a1",
        "en-t-12-abc",
        "en-t-h0-hybrid-h0-other",
    ] {
        assert!(single(code).validate().is_err(), "{code}");
    }
}
