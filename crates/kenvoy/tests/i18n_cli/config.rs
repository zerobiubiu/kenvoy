//! 语言配置只接受明确代码、唯一别名、非空显示名和当前项目协议。

use std::fs;

use super::{Fixture, Result, reject, snapshot, success};

fn languages(fixture: &Fixture, original: &toml::Value, value: &str) -> Result {
    let mut config = original.clone();
    config
        .as_table_mut()
        .ok_or("配置不是表")?
        .insert("languages".into(), toml::from_str(value)?);
    fixture.write_config(&config)
}

fn metadata_only(fixture: &Fixture) -> Result {
    fs::write(
        fixture.project().join("translations.json"),
        r#"{"$schema":"./language.schema.json"}"#,
    )?;
    Ok(())
}

#[test]
fn invalid_language_definitions_fail_without_rewriting_project_or_creating_database() -> Result {
    let fixture = Fixture::new()?;
    let original = fixture.config()?;
    metadata_only(&fixture)?;
    for invalid in [
        "",
        "fallback='zh-CN'",
        "enabled=false\n[zh-CN]\ndisplay='中文'",
        "fallback=''\n[zh-CN]\ndisplay='中文'",
        "fallback='en'\n[en-US]\nalias='en'\ndisplay='English'",
        "fallback='fr-FR'\n[zh-CN]\ndisplay='中文'",
        "[zh-CN]\nalias='cn'",
        "[zh-CN]\ndisplay=''",
        "[zh-CN]\ndisplay='   '",
        "[zh-CN]\ndisplay=3",
        "[zh-CN]\ndisplay='中文'\nalias=''",
        "[zh-CN]\ndisplay='中文'\nalias='  '",
        "[zh-CN]\ndisplay='中文'\nalias=['cn','zh']",
        "[zh-CN]\ndisplay='中文'\nalias='ZH-cn'",
        "[zh-CN]\ndisplay='中文'\nalias='english'\n[en-US]\ndisplay='English'\nalias='ENGLISH'",
        "[zh-CN]\ndisplay='中文'\nalias='EN-us'\n[en-US]\ndisplay='English'",
        "[zh-CN]\ndisplay='中文'\n[zh-cn]\ndisplay='重复代码'",
        "[zh_CN]\ndisplay='非法代码'",
        "[zh-CN]\ndisplay='中文'\n[en--US]\ndisplay='非法代码'",
        "[zh-CN]\ndisplay='中文'\n['../en-US']\ndisplay='非法代码'",
        "[zh-CN]\ndisplay='中文'\nunknown='不支持字段'",
    ] {
        languages(&fixture, &original, invalid)?;
        let before = snapshot(&fixture.project())?;
        for args in [&["check"][..], &["build"][..], &["i18n"][..]] {
            reject(&fixture.command(args)?)?;
            assert_eq!(snapshot(&fixture.project())?, before, "{invalid}");
            assert!(!fixture.project().join("data/app.sqlite").exists());
        }
    }
    fixture.write_config(&original)?;
    success(&fixture.command(&["i18n"])?)?;
    success(&fixture.command(&["check"])?)?;
    Ok(())
}

#[test]
fn omitted_languages_uses_defaults_and_explicit_languages_add_no_implicit_entries() -> Result {
    let fixture = Fixture::new()?;
    metadata_only(&fixture)?;
    let mut original = fixture.config()?;
    original
        .as_table_mut()
        .ok_or("配置不是表")?
        .remove("languages");
    fixture.write_config(&original)?;
    success(&fixture.command(&["i18n"])?)?;
    let path = fixture.project().join("language.schema.json");
    let schema: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(
        schema["additionalProperties"]["properties"]
            .as_object()
            .ok_or("缺少语言定义")?
            .len(),
        6
    );
    for valid in [
        "[zh-CN]\ndisplay='仅中文'",
        "fallback='en-US'\n[en-US]\nalias='english'\ndisplay='Custom English'",
        "fallback='ar'\n[ar]\ndisplay='العربية'",
    ] {
        languages(&fixture, &original, valid)?;
        success(&fixture.command(&["i18n"])?)?;
        success(&fixture.command(&["check"])?)?;
        let schema: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        let count = schema["additionalProperties"]["properties"]
            .as_object()
            .ok_or("缺少语言定义")?
            .len();
        assert_eq!(count, if valid.contains("alias") { 2 } else { 1 });
    }
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn obsolete_commands_and_i18n_configuration_are_rejected_without_compatibility_aliases() -> Result {
    let fixture = Fixture::new()?;
    let before = snapshot(&fixture.project())?;
    for command in ["extract", "check"] {
        reject(&fixture.command(&["i18n", command])?)?;
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    let mut config = fixture.config()?;
    config.as_table_mut().ok_or("配置不是表")?.insert(
        "i18n".into(),
        toml::from_str("default_locale='zh-CN'\nlocales=['zh-CN']\nmessages_dir='locales'")?,
    );
    fixture.write_config(&config)?;
    let before = snapshot(&fixture.project())?;
    for args in [&["check"][..], &["i18n"][..]] {
        reject(&fixture.command(args)?)?;
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    Ok(())
}

#[test]
fn previous_project_formats_and_minimum_cli_declarations_require_manual_conversion() -> Result {
    let fixture = Fixture::new()?;
    let original = fixture.config()?;
    for (format, minimum) in [
        ("0.2.0", "0.2.1"),
        ("0.3.0", "0.3.0"),
        ("0.3.0", "0.4.0"),
        ("0.4.0", "0.3.0"),
    ] {
        let mut config = original.clone();
        config
            .as_table_mut()
            .ok_or("配置不是表")?
            .remove("languages");
        config["project"]["format_version"] = toml::Value::from(format);
        config["project"]["min_cli_version"] = toml::Value::from(minimum);
        fixture.write_config(&config)?;
        let before = snapshot(&fixture.project())?;
        for args in [&["check"][..], &["build"][..], &["i18n"][..]] {
            reject(&fixture.command(args)?)?;
            assert_eq!(
                snapshot(&fixture.project())?,
                before,
                "不能自动升级项目协议"
            );
        }
    }
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn language_configuration_uses_toml_comments_not_javascript_comments() -> Result {
    let fixture = Fixture::new()?;
    let path = fixture.project().join("kenvoy.toml");
    let original = fs::read_to_string(&path)?;
    fs::write(&path, format!("{original}\n# 正确的 TOML 注释\n"))?;
    success(&fixture.command(&["i18n"])?)?;
    fs::write(&path, format!("{original}\n// 非 TOML 注释\n"))?;
    let before = snapshot(&fixture.project())?;
    reject(&fixture.command(&["i18n"])?)?;
    reject(&fixture.command(&["check"])?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
