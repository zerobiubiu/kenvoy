use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{load, messages_dir};
use crate::{
    domain::i18n::{I18nBundle, defaults},
    infrastructure::project::{Project, create},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const EXPLICIT: &str = "[project]\nname='i18n-fixture'\nversion='1.0.0'\nmin_cli_version='0.3.0'\nformat_version='0.3.0'\n[i18n]\ndefault_locale='zh-CN'\nlocales=['zh-CN','en-US']\nmessages_dir='locales'\n";

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("kenvoy-i18n-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, name: &str, value: &str) -> TestResult {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, value)?;
        Ok(())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn legacy_projects_ignore_incidental_catalogs_and_remain_single_chinese() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write(
        "kenvoy.toml",
        "[project]\nversion='1.0.0'\nmin_cli_version='0.2.1'\nformat_version='0.2.0'\n",
    )?;
    scratch.write("locales/en-US.json", "not a dictionary")?;
    let project = Project::load(&scratch.0)?;
    project.check_compatibility()?;
    assert!(project.config.i18n.is_none());
    assert_eq!(messages_dir(&project)?, None);
    assert_eq!(load(&project)?, I18nBundle::default());
    assert_eq!(
        project.config.project.format_version.as_deref(),
        Some("0.2.0")
    );
    assert_eq!(
        project.config.project.min_cli_version.as_deref(),
        Some("0.2.1")
    );
    Ok(())
}

#[test]
fn default_catalog_can_be_missing_without_compiling_or_creating_the_application() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", EXPLICIT)?;
    scratch.write(
        "locales/en-US.json",
        r#"{"app.name":"Customers","custom.count.one":"{count} customer"}"#,
    )?;
    let project = Project::load(&scratch.0)?;
    let bundle = load(&project)?;
    bundle.validate()?;
    assert!(bundle.messages["zh-CN"].is_empty());
    assert_eq!(bundle.messages["en-US"]["app.name"], "Customers");
    assert_eq!(bundle.locales, ["zh-CN", "en-US"]);
    assert!(!project.entry.exists());
    assert!(!project.database.exists());
    assert!(!project.out_dir.exists());
    assert!(!scratch.0.join("locales/zh-CN.json").exists());
    Ok(())
}

#[test]
fn non_default_catalogs_are_required_and_errors_identify_the_language_and_file() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", EXPLICIT)?;
    let project = Project::load(&scratch.0)?;
    let error = load(&project)
        .expect_err("非默认语言不能悄悄消失")
        .to_string();
    assert!(error.contains("非默认语言 en-US"), "{error}");
    assert!(error.contains("en-US.json"), "{error}");
    scratch.write("locales/en-US.json", "{}")?;
    assert!(load(&project)?.messages["en-US"].is_empty());
    Ok(())
}

#[test]
fn malformed_flat_catalogs_fail_without_rewriting_their_contents() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", EXPLICIT)?;
    let project = Project::load(&scratch.0)?;
    for source in [
        r#"{"label":"first","label":"second"}"#,
        r#"{"label":"first","\u006cabel":"second"}"#,
        r#"{"label":{"nested":"value"}}"#,
        r#"{"label":42}"#,
        r#"{"label":null}"#,
        r#"{"":"value"}"#,
        r#"["not","an object"]"#,
        r#"{"label":"unterminated""#,
        r#"{} {}"#,
    ] {
        scratch.write("locales/en-US.json", source)?;
        let error = load(&project).expect_err(source).to_string();
        assert!(error.contains("en-US.json"), "{error}");
        assert_eq!(
            fs::read_to_string(scratch.0.join("locales/en-US.json"))?,
            source
        );
    }
    scratch.write(
        "locales/en-US.json",
        r#"{"label":"first","label":"second"}"#,
    )?;
    let error = load(&project).unwrap_err().to_string();
    assert!(error.contains("重复消息键：label"), "{error}");
    Ok(())
}

#[test]
fn unsafe_directories_and_invalid_language_profiles_are_rejected_at_project_load() -> TestResult {
    let scratch = Scratch::new()?;
    for directory in [
        "../outside",
        ".",
        "data/locales",
        "dist/locales",
        "migrations/locales",
        ".kenvoy/locales",
        "node_modules/locales",
        ".git/locales",
    ] {
        scratch.write(
            "kenvoy.toml",
            &EXPLICIT.replace(
                "messages_dir='locales'",
                &format!("messages_dir='{directory}'"),
            ),
        )?;
        assert!(Project::load(&scratch.0).is_err(), "{directory}");
    }
    for profile in [
        "default_locale='zh-CN'\nlocales=[]",
        "default_locale='zh-CN'\nlocales=['en-US']",
        "default_locale='zh-CN'\nlocales=['zh-CN','zh-cn']",
        "default_locale='zh_CN'\nlocales=['zh_CN']",
        "default_locale='../zh'\nlocales=['../zh']",
    ] {
        let config = EXPLICIT.replace("default_locale='zh-CN'\nlocales=['zh-CN','en-US']", profile);
        scratch.write("kenvoy.toml", &config)?;
        assert!(Project::load(&scratch.0).is_err(), "{profile}");
    }
    scratch.write("kenvoy.toml", EXPLICIT)?;
    scratch.write("locales", "not a directory")?;
    let error = Project::load(&scratch.0).unwrap_err().to_string();
    assert!(error.contains("messages_dir 必须是目录"), "{error}");
    Ok(())
}

#[test]
fn catalogs_can_live_beneath_sources_and_keep_locale_spelling_and_order() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write(
        "kenvoy.toml",
        &EXPLICIT
            .replace("messages_dir='locales'", "messages_dir='src/locales'")
            .replace("default_locale='zh-CN'", "default_locale='en-us'")
            .replace("locales=['zh-CN','en-US']", "locales=['en-us','zh-CN']"),
    )?;
    scratch.write("src/locales/zh-CN.json", r#"{"app.name":"客户"}"#)?;
    let project = Project::load(&scratch.0)?;
    let bundle = load(&project)?;
    assert_eq!(bundle.default_locale, "en-us");
    assert_eq!(bundle.locales, ["en-us", "zh-CN"]);
    assert!(bundle.messages["en-us"].is_empty());
    assert_eq!(
        messages_dir(&project)?,
        Some(scratch.0.join("src/locales").canonicalize()?)
    );
    Ok(())
}

#[test]
fn explicit_i18n_requires_new_format_and_minimum_without_upgrading_old_numbers() -> TestResult {
    let scratch = Scratch::new()?;
    for (minimum, format) in [
        ("0.2.1", "0.3.0"),
        ("0.3.0", "0.2.0"),
        ("0.2.1", "0.2.0"),
        ("0.3.0-rc.1", "0.3.0"),
    ] {
        let config = EXPLICIT
            .replace(
                "min_cli_version='0.3.0'",
                &format!("min_cli_version='{minimum}'"),
            )
            .replace(
                "format_version='0.3.0'",
                &format!("format_version='{format}'"),
            );
        scratch.write("kenvoy.toml", &config)?;
        let project = Project::load(&scratch.0)?;
        assert!(project.check_compatibility().is_err());
        let error = load(&project)
            .expect_err("显式功能必须经过版本门禁")
            .to_string();
        assert!(error.contains("启用 i18n 要求"), "{error}");
        assert_eq!(
            project.config.project.min_cli_version.as_deref(),
            Some(minimum)
        );
        assert_eq!(
            project.config.project.format_version.as_deref(),
            Some(format)
        );
        assert_eq!(fs::read_to_string(scratch.0.join("kenvoy.toml"))?, config);
    }
    Ok(())
}

#[test]
fn newly_created_catalogs_cover_the_actual_customer_application_without_node_or_database()
-> TestResult {
    let scratch = Scratch::new()?;
    let root = scratch.0.join("fresh");
    create(&root)?;
    let project = Project::load(&root)?;
    project.check_compatibility()?;
    let bundle = load(&project)?;
    let update = project.analyze(&mut kengine::AnalysisSession::new())?;
    let (application, _) = crate::application::compilation::compile(
        update.program,
        &project.entry,
        &project.config.project.name,
        &crate::infrastructure::javascript::SwcClientCodeTransformer,
    )?;
    for locale in &bundle.locales {
        for key in defaults(&application).keys() {
            assert!(bundle.messages[locale].contains_key(key), "{locale}: {key}");
        }
    }
    assert_eq!(
        bundle.messages["en-US"]["pages./customers.title"],
        "Customer registration"
    );
    assert_eq!(
        bundle.messages["zh-CN"]["resources.customers.label"],
        "客户"
    );
    assert_eq!(
        project.config.project.format_version.as_deref(),
        Some("0.3.0")
    );
    assert!(!root.join("package.json").exists());
    assert!(!root.join("node_modules").exists());
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn a_development_snapshot_cannot_turn_the_catalog_directory_into_database_storage() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", EXPLICIT)?;
    scratch.write("locales/en-US.json", "{}")?;
    let mut project = Project::load(&scratch.0)?;
    project.use_snapshot(std::path::Path::new("locales/try.sqlite"))?;
    let error = load(&project)
        .expect_err("词典目录不能承载临时业务库")
        .to_string();
    assert!(error.contains("独立于数据库"), "{error}");
    assert!(!project.database.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn catalog_links_cannot_escape_the_root_or_the_catalog_directory() -> TestResult {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new()?;
    let outside = Scratch::new()?;
    scratch.write("kenvoy.toml", EXPLICIT)?;
    outside.write("en-US.json", "{}")?;
    symlink(&outside.0, scratch.0.join("locales"))?;
    assert!(Project::load(&scratch.0).is_err());
    fs::remove_file(scratch.0.join("locales"))?;
    scratch.write("private/en-US.json", "{}")?;
    fs::create_dir(scratch.0.join("locales"))?;
    symlink(
        scratch.0.join("private/en-US.json"),
        scratch.0.join("locales/en-US.json"),
    )?;
    let project = Project::load(&scratch.0)?;
    assert!(load(&project).is_err());
    Ok(())
}
