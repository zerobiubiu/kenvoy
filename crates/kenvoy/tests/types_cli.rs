//! 使用真实 Binary 验证 create/types 的编辑器接线及源码、业务库只读边界。
use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

use serde_json::Value;

#[path = "types_cli/boundaries.rs"]
mod boundaries;
#[path = "types_cli/support.rs"]
mod support;

use support::{Fixture, Result, snapshot, success};

fn json_file(path: impl AsRef<std::path::Path>) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

#[test]
fn create_without_npm_does_not_install_dependencies_or_open_database() -> Result {
    let fixture = Fixture::empty()?;
    let output = fixture.without_tools(&["create", "app"])?;
    success(&output)?;
    assert!(fixture.project().join("migrations").is_dir());
    assert!(!fixture.project().join("data/app.sqlite").exists());
    assert!(!fixture.project().join("node_modules").exists());
    assert!(!fixture.project().join("package-lock.json").exists());
    assert!(!fixture.project().join("dist").exists());
    Ok(())
}

#[test]
fn types_creates_missing_configs_for_custom_entry_without_compiling_source_or_touching_database()
-> Result {
    let fixture = Fixture::new()?;
    for name in ["tsconfig.json", "package.json"] {
        fs::remove_file(fixture.project().join(name))?;
    }
    fs::remove_dir_all(fixture.project().join(".kenvoy/types"))?;
    let config_path = fixture.project().join("kenvoy.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
    config["application"]["entry"] = "features/start/main.tsx".into();
    config["database"]["path"] = "state/live.sqlite".into();
    config["build"]["out_dir"] = "public-output".into();
    let config_bytes = toml::to_string_pretty(&config)?.into_bytes();
    fs::write(&config_path, &config_bytes)?;
    let entry = fixture.project().join("features/start/main.tsx");
    fs::create_dir_all(entry.parent().ok_or("缺少入口目录")?)?;
    let invalid_source = b"throw new Error('must not execute'); export default <app>";
    fs::write(&entry, invalid_source)?;
    fs::create_dir(fixture.project().join("state"))?;
    let database = fixture.project().join("state/live.sqlite");
    let database_bytes = b"not a SQLite database\0owned sentinel";
    fs::write(&database, database_bytes)?;
    fs::write(
        fixture.project().join("translations.json"),
        "这也不是合法 JSON",
    )?;
    let migrations = snapshot(&fixture.project().join("migrations"))?;
    let output = fixture.without_tools(&["--project", "app/features/start", "types"])?;
    success(&output)?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["status"], "declarations_generated");
    assert_eq!(report["tsconfig"]["state"], "created");
    assert_eq!(report["package"]["state"], "created");
    assert_eq!(report["dependency_installation_performed"], false);
    assert_eq!(report["configuration_review_required"], false);
    assert_eq!(fs::read(&entry)?, invalid_source);
    assert_eq!(fs::read(&database)?, database_bytes);
    assert_eq!(fs::read(config_path)?, config_bytes);
    assert_eq!(snapshot(&fixture.project().join("migrations"))?, migrations);
    assert_eq!(
        fs::read_to_string(fixture.project().join("translations.json"))?,
        "这也不是合法 JSON"
    );
    fs::remove_file(entry)?;
    success(&fixture.types()?)?;
    assert_eq!(fs::read(database)?, database_bytes);
    assert_eq!(snapshot(&fixture.project().join("migrations"))?, migrations);
    assert!(!fixture.project().join("node_modules").exists());
    assert!(!fixture.project().join("package-lock.json").exists());
    assert!(!fixture.project().join("public-output").exists());
    assert!(!fixture.project().join("state/live.sqlite-wal").exists());
    Ok(())
}

#[test]
fn types_preserves_valid_and_broken_existing_configuration_bytes_and_reports_manual_work() -> Result
{
    let fixture = Fixture::new()?;
    let cases: &[(&[u8], &[u8])] = &[
        (
            b"{\r\n  \"compilerOptions\": {\"strict\": false, \"jsx\": \"preserve\"}\r\n}\r\n",
            b"{\r\n  \"private\": true, \"scripts\": {\"postinstall\": \"exit 97\"}, \"dependencies\": {\"user-library\": \"1.0.0\"}\r\n}\r\n",
        ),
        (b"{ broken JSON\xff", b"\0also not JSON\xff"),
    ];
    for &(tsconfig, package) in cases {
        fs::write(fixture.project().join("tsconfig.json"), tsconfig)?;
        fs::write(fixture.project().join("package.json"), package)?;
        let before = snapshot(&fixture.project())?;
        let output = fixture.types()?;
        success(&output)?;
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["tsconfig"]["state"], "preserved");
        assert_eq!(report["package"]["state"], "preserved");
        assert_eq!(report["configuration_review_required"], true);
        assert_eq!(report["dependency_installation_performed"], false);
        assert_eq!(snapshot(&fixture.project())?, before);
    }
    Ok(())
}

#[test]
fn types_only_creates_the_missing_one_of_two_configuration_files() -> Result {
    for preserved in ["tsconfig.json", "package.json"] {
        let fixture = Fixture::new()?;
        let missing = if preserved == "tsconfig.json" {
            "package.json"
        } else {
            "tsconfig.json"
        };
        let marker = b"invalid JSON that belongs to the user\r\n";
        fs::write(fixture.project().join(preserved), marker)?;
        fs::remove_file(fixture.project().join(missing))?;
        let output = fixture.types()?;
        success(&output)?;
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let kept_field = if preserved == "tsconfig.json" {
            "tsconfig"
        } else {
            "package"
        };
        let new_field = if missing == "tsconfig.json" {
            "tsconfig"
        } else {
            "package"
        };
        assert_eq!(report[kept_field]["state"], "preserved");
        assert_eq!(report[new_field]["state"], "created");
        assert_eq!(fs::read(fixture.project().join(preserved))?, marker);
        assert!(json_file(fixture.project().join(missing))?.is_object());
        assert!(!fixture.project().join("data/app.sqlite").exists());
    }
    Ok(())
}

#[test]
fn repeated_types_keeps_generated_mtimes_and_additional_user_files() -> Result {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.project().join(".kenvoy/types/user.d.ts"),
        "export interface UserOwned { name: string }\n",
    )?;
    fs::write(
        fixture.project().join(".kenvoy/types/keep.txt"),
        "用户文件不在生成清单内",
    )?;
    let before = snapshot(&fixture.project())?;
    let old_time = UNIX_EPOCH + Duration::from_secs(86_400);
    for path in before.keys() {
        fs::OpenOptions::new()
            .write(true)
            .open(fixture.project().join(path))?
            .set_modified(old_time)?;
    }
    let output = fixture.types()?;
    success(&output)?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["declarations"]["created"], 0);
    assert_eq!(report["declarations"]["updated"], 0);
    assert_eq!(report["declarations"]["removed"], 0);
    assert_eq!(
        report["declarations"]["unchanged"],
        report["declarations"]["total"]
    );
    assert_eq!(snapshot(&fixture.project())?, before);
    for path in before.keys() {
        assert_eq!(
            fs::metadata(fixture.project().join(path))?.modified()?,
            old_time,
            "{}",
            path.display()
        );
    }
    Ok(())
}
