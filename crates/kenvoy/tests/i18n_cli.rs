//! 用真实 Binary 验证强制集中译文、Schema 生命周期与只读边界。

use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

#[path = "i18n_cli/config.rs"]
mod config;
#[path = "i18n_cli/translations.rs"]
mod translations;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-i18n-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        let fixture = Self(path);
        success(&fixture.run(&["create", "app"])?)?;
        Ok(fixture)
    }
    fn project(&self) -> PathBuf {
        self.0.join("app")
    }
    fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .output()?)
    }
    fn command(&self, args: &[&str]) -> Result<Output> {
        let mut command = vec!["--project", "app"];
        command.extend_from_slice(args);
        self.run(&command)
    }
    fn config(&self) -> Result<toml::Value> {
        Ok(toml::from_str(&fs::read_to_string(
            self.project().join("kenvoy.toml"),
        )?)?)
    }
    fn write_config(&self, config: &toml::Value) -> Result {
        fs::write(
            self.project().join("kenvoy.toml"),
            toml::to_string_pretty(config)?,
        )?;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn success(output: &Output) -> Result {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
fn reject(output: &Output) -> Result<String> {
    assert!(!output.status.success(), "不应接受该译文、配置或命令");
    let diagnostic = String::from_utf8(output.stderr.clone())?;
    assert!(!diagnostic.trim().is_empty(), "拒绝必须提供诊断");
    Ok(diagnostic)
}
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            assert!(!kind.is_symlink(), "自有夹具不应包含符号链接");
            if kind.is_dir() {
                visit(root, &entry.path(), files)?;
            } else {
                files.insert(
                    entry.path().strip_prefix(root)?.into(),
                    fs::read(entry.path())?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

#[test]
fn create_provides_required_root_files_and_keeps_application_ir_format_one() -> Result {
    let fixture = Fixture::new()?;
    let config = fixture.config()?;
    assert_eq!(config["project"]["format_version"].as_str(), Some("0.4.0"));
    assert_eq!(config["project"]["min_cli_version"].as_str(), Some("0.4.0"));
    assert!(config.get("i18n").is_none());
    assert!(!fixture.project().join("locales").exists());
    let translations: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("translations.json"))?)?;
    assert_eq!(translations["$schema"], "./language.schema.json");
    success(&fixture.command(&["i18n"])?)?;
    let schema: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("language.schema.json"))?)?;
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["required"], json!(["$schema"]));
    assert_eq!(schema["minProperties"], 1);
    assert_eq!(
        schema["properties"]["$schema"]["const"],
        "./language.schema.json"
    );
    for key in ["zh-CN", "cn", "en-US", "en", "ko-KR", "ko"] {
        assert_eq!(
            schema["additionalProperties"]["properties"][key]["type"],
            "string"
        );
        assert_eq!(
            schema["additionalProperties"]["properties"][key]["minLength"],
            1
        );
    }
    success(&fixture.command(&["check"])?)?;
    let inspected = fixture.command(&["inspect"])?;
    success(&inspected)?;
    let ir: Value = serde_json::from_slice(&inspected.stdout)?;
    assert_eq!(ir["format_version"], 1);
    assert_eq!(ir["resources"][0]["name"], "customers");
    assert!(!fixture.project().join("data/app.sqlite").exists());
    assert!(!fixture.project().join("dist").exists());
    Ok(())
}

#[test]
fn first_i18n_creates_only_schema_reference_without_reading_source_or_database() -> Result {
    let fixture = Fixture::new()?;
    fs::remove_file(fixture.project().join("translations.json"))?;
    fs::rename(
        fixture.project().join("src"),
        fixture.0.join("source-archive"),
    )?;
    let schema = fixture.project().join("language.schema.json");
    if schema.exists() {
        fs::remove_file(&schema)?;
    }
    let database = fixture.project().join("data/app.sqlite");
    let marker = b"not-a-sqlite-database\0owned-marker";
    fs::write(&database, marker)?;
    let initialized = fixture.command(&["i18n"])?;
    success(&initialized)?;
    let report: Value = serde_json::from_slice(&initialized.stdout)?;
    assert_eq!(report["status"], "passed");
    assert_eq!(report["schema"], "language.schema.json");
    assert_eq!(report["translations"], "translations.json");
    assert_eq!(report["schema_updated"], true);
    assert_eq!(report["translations_created"], true);
    assert_eq!(report["message_count"], 0);
    assert_eq!(report["fallback"], "zh-CN");
    assert_eq!(
        report["languages"].as_array().ok_or("缺少语言列表")?.len(),
        3
    );
    let translations: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("translations.json"))?)?;
    assert_eq!(translations, json!({"$schema":"./language.schema.json"}));
    assert!(schema.is_file());
    assert_eq!(fs::read(database)?, marker);
    assert!(!fixture.project().join("dist").exists());
    Ok(())
}

#[test]
fn i18n_preserves_existing_bytes_and_schema_mtime_when_configuration_is_unchanged() -> Result {
    let fixture = Fixture::new()?;
    let path = fixture.project().join("translations.json");
    let original = b"{\r\n  \"$schema\": \"./language.schema.json\",\r\n  \"custom.title\": {\"zh-CN\":\"Chinese\", \"en\":\"English\", \"ko-KR\":\"Korean\"}\r\n}\r\n";
    fs::write(&path, original)?;
    success(&fixture.command(&["i18n"])?)?;
    let schema_path = fixture.project().join("language.schema.json");
    let old_time = UNIX_EPOCH + std::time::Duration::from_secs(86_400);
    for file in [&path, &schema_path] {
        fs::File::options()
            .write(true)
            .open(file)?
            .set_times(fs::FileTimes::new().set_modified(old_time))?;
    }
    let schema_time = fs::metadata(&schema_path)?.modified()?;
    let translation_time = fs::metadata(&path)?.modified()?;
    let before = snapshot(&fixture.project())?;
    let repeated = fixture.command(&["i18n"])?;
    success(&repeated)?;
    let report: Value = serde_json::from_slice(&repeated.stdout)?;
    assert_eq!(report["schema_updated"], false);
    assert_eq!(report["translations_created"], false);
    assert_eq!(report["message_count"], 1);
    success(&fixture.command(&["check"])?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    assert_eq!(fs::read(&path)?, original);
    assert_eq!(fs::metadata(&schema_path)?.modified()?, schema_time);
    assert_eq!(fs::metadata(&path)?.modified()?, translation_time);
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn i18n_updates_schema_for_configuration_without_rewriting_existing_translations() -> Result {
    let fixture = Fixture::new()?;
    let path = fixture.project().join("translations.json");
    let original = r#"{"$schema":"./language.schema.json","title":{"zh-CN":"标题","en-US":"Title","ko-KR":"제목"}}"#;
    fs::write(&path, original)?;
    success(&fixture.command(&["i18n"])?)?;
    let schema_path = fixture.project().join("language.schema.json");
    let previous = fs::read(&schema_path)?;
    let mut config = fixture.config()?;
    config["languages"] = toml::from_str::<toml::Value>(
        "fallback='zh-CN'\n[zh-CN]\nalias='china'\ndisplay='中文标题'\n[en-US]\nalias='en'\ndisplay='English'\n[ko-KR]\nalias='ko'\ndisplay='한국어'\n",
    )?;
    fixture.write_config(&config)?;
    success(&fixture.command(&["i18n"])?)?;
    let changed = fs::read(&schema_path)?;
    assert_ne!(changed, previous);
    let schema: Value = serde_json::from_slice(&changed)?;
    let keys = &schema["additionalProperties"]["properties"];
    assert!(keys.get("china").is_some());
    assert!(keys.get("cn").is_none());
    assert_eq!(fs::read_to_string(&path)?, original);
    success(&fixture.command(&["i18n"])?)?;
    assert_eq!(fs::read(&schema_path)?, changed);
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
