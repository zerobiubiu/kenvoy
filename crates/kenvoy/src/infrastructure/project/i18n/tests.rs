use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{load, prepare, schema};
use crate::{
    domain::i18n::I18nBundle,
    infrastructure::project::{Project, create},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT: &str = "[project]\nname='i18n-fixture'\nversion='1.0.0'\nmin_cli_version='0.4.0'\nformat_version='0.4.0'\n";
const METADATA: &str = r#"{"$schema":"./language.schema.json"}"#;
const TRANSLATIONS: &str = r#"{
  "$schema": "./language.schema.json",
  "app.name": {"cn": "客户", "en-US": "Customers", "ko": "고객"},
  "custom.count": {"zh-CN": "{count} 位客户", "en": "{count} customers", "ko-KR": "고객 {count}명"}
}"#;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("kenvoy-i18n-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn write(&self, name: &str, value: &str) -> TestResult {
        fs::write(self.0.join(name), value)?;
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
fn omitted_languages_still_require_root_translations_without_creating_anything() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let project = Project::load(&scratch.0)?;
    assert_eq!(project.config.languages.definitions.len(), 3);
    let error = load(&project).unwrap_err().to_string();
    assert!(error.contains("translations.json"), "{error}");
    assert!(error.contains("kenvoy i18n"), "{error}");
    assert_eq!(fs::read_dir(&scratch.0)?.count(), 1);
    scratch.write("translations.json", METADATA)?;
    assert_eq!(load(&project)?, I18nBundle::default());
    assert!(!project.entry.exists());
    assert!(!project.database.exists());
    assert!(!project.out_dir.exists());
    assert!(!scratch.0.join("language.schema.json").exists());
    Ok(())
}

#[test]
fn read_only_load_normalizes_each_alias_to_code_without_sources_or_schema_file() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    scratch.write("translations.json", TRANSLATIONS)?;
    let project = Project::load(&scratch.0)?;
    let bundle = load(&project)?;
    bundle.validate()?;
    assert_eq!(bundle.messages["zh-CN"]["app.name"], "客户");
    assert_eq!(bundle.messages["en-US"]["app.name"], "Customers");
    assert_eq!(bundle.messages["ko-KR"]["custom.count"], "고객 {count}명");
    assert!(!bundle.messages.contains_key("cn"));
    assert_eq!(
        fs::read_to_string(scratch.0.join("translations.json"))?,
        TRANSLATIONS
    );
    assert!(!scratch.0.join("language.schema.json").exists());
    assert!(!project.entry.exists());
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn prepare_creates_only_metadata_and_preserves_identical_schema_mtime() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let project = Project::load(&scratch.0)?;
    let first = prepare(&project, true)?;
    assert!(first.schema_updated);
    assert!(first.translations_created);
    assert_eq!(first.bundle, I18nBundle::default());
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(scratch.0.join("translations.json"))?)?;
    assert_eq!(
        document,
        serde_json::json!({"$schema": "./language.schema.json"})
    );
    let schema_path = scratch.0.join("language.schema.json");
    let generated = fs::read(&schema_path)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&generated)?,
        schema(&project.config.languages)?
    );
    assert_eq!(generated.last(), Some(&b'\n'));
    let modified = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    fs::OpenOptions::new()
        .write(true)
        .open(&schema_path)?
        .set_times(fs::FileTimes::new().set_modified(modified))?;
    let before = fs::metadata(&schema_path)?.modified()?;
    let second = prepare(&project, true)?;
    assert!(!second.schema_updated);
    assert!(!second.translations_created);
    assert_eq!(fs::metadata(&schema_path)?.modified()?, before);
    assert!(!project.entry.exists());
    assert!(!project.database.exists());
    assert!(!project.out_dir.exists());
    Ok(())
}

#[test]
fn prepare_without_creation_refreshes_schema_then_reports_missing_translations() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let project = Project::load(&scratch.0)?;
    let error = prepare(&project, false).unwrap_err().to_string();
    assert!(error.contains("kenvoy i18n"), "{error}");
    assert!(scratch.0.join("language.schema.json").is_file());
    assert!(!scratch.0.join("translations.json").exists());
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn schema_refresh_never_overwrites_existing_valid_or_invalid_translation_bytes() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    scratch.write("translations.json", TRANSLATIONS)?;
    let project = Project::load(&scratch.0)?;
    let first = prepare(&project, true)?;
    assert!(first.schema_updated);
    assert!(!first.translations_created);
    assert_eq!(
        fs::read_to_string(scratch.0.join("translations.json"))?,
        TRANSLATIONS
    );
    let previous_schema = fs::read(scratch.0.join("language.schema.json"))?;
    let mut changed = project.clone();
    changed
        .config
        .languages
        .definitions
        .get_mut("zh-CN")
        .unwrap()
        .alias = Some("chinese".into());
    assert!(prepare(&changed, true).is_err());
    assert_ne!(
        fs::read(scratch.0.join("language.schema.json"))?,
        previous_schema
    );
    assert_eq!(
        fs::read_to_string(scratch.0.join("translations.json"))?,
        TRANSLATIONS
    );
    scratch.write("translations.json", "invalid user text")?;
    assert!(prepare(&project, true).is_err());
    assert_eq!(
        fs::read_to_string(scratch.0.join("translations.json"))?,
        "invalid user text"
    );
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn malformed_duplicate_or_schema_invalid_json_never_changes_input_files() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let project = Project::load(&scratch.0)?;
    for source in [
        r#"{}"#,
        r#"{"$schema":"wrong"}"#,
        r#"{"$schema":"./language.schema.json","$schema":"./language.schema.json"}"#,
        r#"{"$schema":"./language.schema.json","\u0024schema":"./language.schema.json"}"#,
        r#"{"$schema":"./language.schema.json","label":{},"\u006cabel":{}}"#,
        r#"{"$schema":"./language.schema.json","label":{"cn":"甲","cn":"乙"}}"#,
        r#"{"$schema":"./language.schema.json","label":{"cn":"甲","\u0063n":"乙"}}"#,
        r#"{"$schema":"./language.schema.json","label":{"cn":"甲","en":"A"}}"#,
        r#"{"$schema":"./language.schema.json","label":{"cn":"甲","en":"A","ko":"","zh-CN":"乙"}}"#,
        r#"{"$schema":"./language.schema.json","label":null}"#,
        r#"["not","an object"]"#,
        r#"{"label":"unterminated""#,
        r#"{} {}"#,
    ] {
        scratch.write("translations.json", source)?;
        let error = load(&project).expect_err(source).to_string();
        assert!(error.contains("translations.json"), "{error}");
        if source.contains("\\u0063n") {
            assert!(error.contains("重复 JSON 键：cn"), "{error}");
        }
        assert_eq!(
            fs::read_to_string(scratch.0.join("translations.json"))?,
            source
        );
        assert!(!scratch.0.join("language.schema.json").exists());
    }
    Ok(())
}

#[test]
fn invalid_language_configuration_fails_before_schema_or_translation_writes() -> TestResult {
    let scratch = Scratch::new()?;
    for languages in [
        "[languages]",
        "[languages]\nfallback='cn'\n[languages.zh-CN]\ndisplay='中文'\nalias='cn'",
        "[languages]\n[languages.en-US]\ndisplay='English'",
        "[languages]\n[languages.zh-CN]\ndisplay='中文'\nalias='ZH-cn'",
        "[languages]\n[languages.zh-CN]\ndisplay='中文'\n[languages.zh-cn]\ndisplay='中文'",
        "[languages]\n[languages.zh-CN]\ndisplay=''",
        "[languages]\n[languages.zh-CN]\nalias='cn'",
        "[languages]\n[languages.zh-CN]\ndisplay='中文'\nextra='x'",
        "[languages]\n[languages.zh-CN]\ndisplay='中文'\nalias=['cn','zh']",
        "[languages]\n[languages.zh_CN]\ndisplay='中文'",
        "[languages]\n// invalid TOML comment",
        "[i18n]\ndefault_locale='zh-CN'\nlocales=['zh-CN']",
    ] {
        scratch.write("kenvoy.toml", &format!("{PROJECT}{languages}\n"))?;
        let error = Project::load(&scratch.0).expect_err(languages).to_string();
        if languages.starts_with("[i18n]") {
            assert!(error.contains("手工转换"), "{error}");
        }
        assert_eq!(fs::read_dir(&scratch.0)?.count(), 1);
    }
    scratch.write("kenvoy.toml", PROJECT)?;
    let mut project = Project::load(&scratch.0)?;
    project.config.languages.definitions.clear();
    assert!(prepare(&project, true).is_err());
    assert_eq!(fs::read_dir(&scratch.0)?.count(), 1);
    Ok(())
}

#[test]
fn explicit_single_language_does_not_add_defaults_and_keeps_declared_spelling() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write(
        "kenvoy.toml",
        &format!(
            "{PROJECT}[languages]\nfallback='en-us'\n[languages.en-us]\ndisplay='Custom English'\n"
        ),
    )?;
    scratch.write(
        "translations.json",
        r#"{"$schema":"./language.schema.json","title":{"en-us":"Hello"}}"#,
    )?;
    let project = Project::load(&scratch.0)?;
    let bundle = load(&project)?;
    assert_eq!(bundle.fallback, "en-us");
    assert_eq!(bundle.languages.len(), 1);
    assert_eq!(bundle.languages[0].display, "Custom English");
    assert_eq!(bundle.languages[0].alias, None);
    assert_eq!(bundle.messages["en-us"]["title"], "Hello");
    Ok(())
}

#[test]
fn old_project_versions_are_rejected_before_prepare_writes() -> TestResult {
    let scratch = Scratch::new()?;
    for (minimum, format) in [
        ("0.3.0", "0.4.0"),
        ("0.4.0", "0.3.0"),
        ("0.4.0-rc.1", "0.4.0"),
    ] {
        let source = PROJECT
            .replace(
                "min_cli_version='0.4.0'",
                &format!("min_cli_version='{minimum}'"),
            )
            .replace(
                "format_version='0.4.0'",
                &format!("format_version='{format}'"),
            );
        scratch.write("kenvoy.toml", &source)?;
        let project = Project::load(&scratch.0)?;
        assert!(load(&project).is_err());
        assert!(prepare(&project, true).is_err());
        assert_eq!(fs::read_dir(&scratch.0)?.count(), 1);
        assert_eq!(fs::read_to_string(scratch.0.join("kenvoy.toml"))?, source);
    }
    Ok(())
}

#[test]
fn newly_created_template_contains_complete_accurate_business_translations() -> TestResult {
    let scratch = Scratch::new()?;
    let root = scratch.0.join("fresh");
    create(&root)?;
    let project = Project::load(&root)?;
    let bundle = load(&project)?;
    for language in &bundle.languages {
        assert_eq!(bundle.messages[&language.code].len(), 6);
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
        bundle.messages["ko-KR"]["resources.customers.fields.age.label"],
        "나이"
    );
    assert_eq!(
        project.config.project.format_version.as_deref(),
        Some("0.4.0")
    );
    assert!(root.join("language.schema.json").is_file());
    assert!(!root.join("locales").exists());
    assert!(!root.join("node_modules").exists());
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn regular_file_and_size_limits_fail_without_creating_database() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let project = Project::load(&scratch.0)?;
    fs::create_dir(scratch.0.join("translations.json"))?;
    assert!(load(&project).is_err());
    assert!(prepare(&project, true).is_err());
    fs::remove_dir(scratch.0.join("translations.json"))?;
    fs::File::create(scratch.0.join("translations.json"))?.set_len(32 * 1024 * 1024 + 1)?;
    assert!(load(&project).unwrap_err().to_string().contains("32 MiB"));
    fs::remove_file(scratch.0.join("language.schema.json"))?;
    fs::create_dir(scratch.0.join("language.schema.json"))?;
    assert!(prepare(&project, true).is_err());
    assert!(!project.database.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn translations_and_schema_reject_internal_external_and_dangling_links() -> TestResult {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new()?;
    let outside = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    scratch.write("private.json", METADATA)?;
    outside.write("outside.json", METADATA)?;
    let project = Project::load(&scratch.0)?;
    for target in [
        scratch.0.join("private.json"),
        outside.0.join("outside.json"),
        outside.0.join("missing.json"),
    ] {
        symlink(&target, scratch.0.join("translations.json"))?;
        assert!(load(&project).is_err());
        assert!(prepare(&project, true).is_err());
        fs::remove_file(scratch.0.join("translations.json"))?;
        fs::remove_file(scratch.0.join("language.schema.json"))?;
        symlink(&target, scratch.0.join("language.schema.json"))?;
        assert!(prepare(&project, true).is_err());
        fs::remove_file(scratch.0.join("language.schema.json"))?;
    }
    assert_eq!(
        fs::read_to_string(outside.0.join("outside.json"))?,
        METADATA
    );
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn schema_diagnostic_identifies_the_location_without_echoing_translation_values() -> TestResult {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", PROJECT)?;
    let text = r#"{"$schema":"./language.schema.json","private_note":{"cn":"TRANSLATION_CONTENT_NOT_FOR_LOGS","zh-CN":"重复","en":"Text","ko":"문구"}}"#;
    scratch.write("translations.json", text)?;
    let error = load(&Project::load(&scratch.0)?).unwrap_err().to_string();
    assert!(error.contains("/private_note"), "{error}");
    assert!(error.contains("oneOf"), "{error}");
    assert!(!error.contains("TRANSLATION_CONTENT_NOT_FOR_LOGS"));
    assert_eq!(
        fs::read_to_string(scratch.0.join("translations.json"))?,
        text
    );
    Ok(())
}
