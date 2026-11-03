//! 词典输入、路径、语言与项目格式边界。

use std::fs;

use serde_json::{Value, json};

use super::{Fixture, Result, reject, snapshot, success};

#[test]
fn check_rejects_duplicate_keys_non_string_and_malformed_catalogs_without_writes() -> Result {
    let fixture = Fixture::new()?;
    let dictionary = fixture.project().join("locales/en-US.json");
    for invalid in [
        r#"{"app.name":"First","app.name":"Second"}"#,
        r#"{"custom.count":3}"#,
        r#"{"custom.nested":{"value":"no"}}"#,
        r#"["not a message map"]"#,
        r#"{"app.name":"unterminated"#,
    ] {
        fs::write(&dictionary, invalid)?;
        let before = snapshot(&fixture.project())?;
        for args in [vec!["check"], vec!["i18n", "check"]] {
            reject(&fixture.command(&args)?, "en-US.json")?;
        }
        assert_eq!(snapshot(&fixture.project())?, before);
        assert!(!fixture.project().join("data/app.sqlite").exists());
    }
    Ok(())
}

#[test]
fn check_rejects_catalog_directories_excluded_from_fingerprints_and_watchers() -> Result {
    let fixture = Fixture::new()?;
    let original = fixture.config()?;
    success(&fixture.command(&["i18n", "check"])?)?;
    for directory in [
        "target/locales",
        ".env-locales",
        "src/target/locales",
        "src/.env-i18n",
    ] {
        let destination = fixture.project().join(directory);
        fs::create_dir_all(&destination)?;
        for locale in ["zh-CN", "en-US"] {
            fs::copy(
                fixture
                    .project()
                    .join("locales")
                    .join(format!("{locale}.json")),
                destination.join(format!("{locale}.json")),
            )?;
        }
        let mut config = original.clone();
        config["i18n"]["messages_dir"] = toml::Value::from(directory);
        fixture.write_config(&config)?;
        let before = snapshot(&fixture.project())?;
        reject(&fixture.command(&["i18n", "check"])?, "messages_dir")?;
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn check_enforces_locale_and_directory_boundaries_but_allows_source_locales() -> Result {
    let fixture = Fixture::new()?;
    let original = fixture.config()?;
    for directory in [
        "../outside",
        ".",
        "data",
        "data/locales",
        "dist",
        "dist/locales",
        "migrations",
        "migrations/locales",
    ] {
        let mut config = original.clone();
        config["i18n"]["messages_dir"] = toml::Value::from(directory);
        fixture.write_config(&config)?;
        let before = snapshot(&fixture.project())?;
        reject(&fixture.command(&["check"])?, "messages_dir")?;
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    let mut absolute = original.clone();
    absolute["i18n"]["messages_dir"] = toml::Value::from(fixture.0.to_string_lossy().into_owned());
    fixture.write_config(&absolute)?;
    let before = snapshot(&fixture.project())?;
    reject(&fixture.command(&["check"])?, "messages_dir")?;
    assert_eq!(snapshot(&fixture.project())?, before);
    for (default, locales) in [
        ("zh-CN", vec!["zh-CN", "zh-cn"]),
        ("zh-CN", vec!["zh_CN", "en-US"]),
        ("zh-CN", vec!["zh-CN", "en--US"]),
        ("zh-CN", vec!["zh-CN", ""]),
        ("fr-FR", vec!["zh-CN", "en-US"]),
        ("zh-CN", vec!["zh-CN", "../en-US"]),
        ("zh-CN", Vec::new()),
    ] {
        let mut config = original.clone();
        config["i18n"]["default_locale"] = toml::Value::from(default);
        config["i18n"]["locales"] =
            toml::Value::Array(locales.into_iter().map(toml::Value::from).collect());
        fixture.write_config(&config)?;
        reject(&fixture.command(&["check"])?, "i18n")?;
    }
    let mut config = original;
    config["i18n"]["messages_dir"] = toml::Value::from("src/locales");
    fixture.write_config(&config)?;
    fs::rename(
        fixture.project().join("locales"),
        fixture.project().join("src/locales"),
    )?;
    success(&fixture.command(&["check"])?)?;
    success(&fixture.command(&["i18n", "check"])?)?;
    assert_eq!(
        fixture.extract()?["resources.customers.fields.name.label"],
        "客户姓名"
    );
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn missing_default_uses_dsl_fallback_and_missing_nondefault_is_an_error() -> Result {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.project().join("locales/zh-CN.json"))?;
    success(&fixture.command(&["check"])?)?;
    let checked = fixture.command(&["i18n", "check"])?;
    success(&checked)?;
    let report: Value = serde_json::from_slice(&checked.stdout)?;
    assert_eq!(report["status"], "passed");
    assert_eq!(report["default_locale"], "zh-CN");
    assert_eq!(report["locales"], json!(["zh-CN", "en-US"]));
    assert!(
        report["missing"]["zh-CN"]
            .as_array()
            .ok_or("缺少默认语言缺失报告")?
            .contains(&json!("resources.customers.fields.name.label"))
    );
    assert_eq!(fixture.extract()?["pages./customers.title"], "客户登记");
    fs::remove_file(fixture.project().join("locales/en-US.json"))?;
    reject(&fixture.command(&["check"])?, "en-US")?;
    reject(&fixture.command(&["i18n", "check"])?, "en-US")?;
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn legacy_format_accepts_new_cli_but_explicit_i18n_needs_both_new_version_declarations() -> Result {
    let fixture = Fixture::new()?;
    let original = fixture.config()?;
    for (format, minimum) in [("0.2.0", "0.2.1"), ("0.2.0", "0.3.0"), ("0.3.0", "0.2.1")] {
        let mut config = original.clone();
        config["project"]["format_version"] = toml::Value::from(format);
        config["project"]["min_cli_version"] = toml::Value::from(minimum);
        fixture.write_config(&config)?;
        let before = snapshot(&fixture.project())?;
        for args in [
            vec!["check"],
            vec!["i18n", "extract"],
            vec!["i18n", "check"],
        ] {
            reject(&fixture.command(&args)?, "i18n")?;
        }
        assert_eq!(
            snapshot(&fixture.project())?,
            before,
            "不能自动升级配置数字"
        );
    }
    let mut legacy = original;
    legacy.as_table_mut().ok_or("配置不是表")?.remove("i18n");
    legacy["project"]["format_version"] = toml::Value::from("0.2.0");
    legacy["project"]["min_cli_version"] = toml::Value::from("0.2.1");
    fixture.write_config(&legacy)?;
    let before = snapshot(&fixture.project())?;
    success(&fixture.command(&["check"])?)?;
    let output = fixture.command(&["inspect"])?;
    success(&output)?;
    let application: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(application["format_version"], 1);
    assert_eq!(application["name"], "客户管理系统");
    assert_eq!(
        fixture.extract()?["pages./customers.title"],
        json!("客户登记")
    );
    assert_eq!(snapshot(&fixture.project())?, before);
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
