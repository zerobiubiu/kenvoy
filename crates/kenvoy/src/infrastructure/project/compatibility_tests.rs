use super::{Project, compatibility::check, config::ProjectConfig};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn config(minimum: &str, format: &str) -> ProjectConfig {
    ProjectConfig {
        name: "版本边界夹具".into(),
        version: "1.2.3".into(),
        min_cli_version: Some(minimum.into()),
        format_version: Some(format.into()),
    }
}

#[test]
fn full_semver_is_required_for_every_project_version_field() {
    for invalid in [
        "",
        "1",
        "1.2",
        "v1.2.3",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-01",
        "1.2.3+",
        "1.2.3-alpha_1",
        " 1.2.3",
        "1.2.3 ",
        ">=1.2.3",
        "18446744073709551616.0.0",
    ] {
        let mut project = config("0.2.1", "0.2.0");
        project.version = invalid.into();
        assert!(
            check(&project, "0.2.1").is_err(),
            "project.version={invalid:?}"
        );
        let mut project = config(invalid, "0.2.0");
        assert!(
            check(&project, "0.2.1").is_err(),
            "min_cli_version={invalid:?}"
        );
        project = config("0.2.1", invalid);
        assert!(
            check(&project, "0.2.1").is_err(),
            "format_version={invalid:?}"
        );
    }
    for valid in [
        "0.0.0",
        "1.2.3-alpha.1",
        "1.2.3+build.9",
        "1.2.3-rc.1+build",
    ] {
        let mut project = config("0.2.1", "0.2.0");
        project.version = valid.into();
        assert!(check(&project, "0.2.1").is_ok(), "{valid}");
    }
}

#[test]
fn minimum_cli_uses_semver_precedence_not_strings_or_build_metadata() {
    for (cli, minimum, accepted) in [
        ("0.2.1", "0.2.1", true),
        ("0.2.9", "0.2.10", false),
        ("0.2.10", "0.2.9", true),
        ("0.3.0", "0.2.99", true),
        ("0.2.1+local", "0.2.1+release", true),
        ("0.2.1-rc.1", "0.2.1", false),
        ("0.2.1-rc.1", "0.2.1-rc.1", true),
        ("0.2.1-beta.2", "0.2.1-beta.11", false),
        ("0.2.1-beta.11", "0.2.1-beta.2", true),
        ("0.2.1-alpha", "0.2.1-beta", false),
        ("0.2.1-rc.1+local", "0.2.1-rc.1+release", true),
        ("0.2.2-alpha", "0.2.1", true),
        ("0.2.1", "0.2.1-rc.1", false),
        ("1.0.0", "0.2.1-rc.1", false),
    ] {
        assert_eq!(
            check(&config(minimum, "0.2.0"), cli).is_ok(),
            accepted,
            "CLI={cli}, minimum={minimum}"
        );
    }
}

#[test]
fn format_gate_is_independent_from_minimum_cli_gate() {
    for (cli, format, accepted) in [
        ("0.2.1", "0.1.99", false),
        ("0.2.1", "0.2.0", true),
        ("0.2.1", "0.2.999", true),
        ("0.3.0", "0.3.0", true),
        ("0.3.0", "0.3.999", true),
        ("0.3.0", "0.4.0", false),
        ("0.2.1", "1.0.0", false),
        ("0.2.1", "0.2.0+schema.1", true),
        ("0.2.1", "0.2.1-alpha", false),
        ("0.2.1-alpha", "0.2.0-alpha", false),
        ("0.2.1-alpha", "0.2.1-alpha", true),
        ("0.2.1-alpha", "0.3.0-alpha", true),
        ("0.3.0-alpha", "0.4.0", false),
    ] {
        assert_eq!(
            check(&config("0.2.0", format), cli).is_ok(),
            accepted,
            "CLI={cli}, format={format}"
        );
    }
    let error = check(&config("0.9.0", "0.4.0"), "0.3.0")
        .expect_err("两个门禁必须同时报告")
        .to_string();
    assert!(error.contains("低于项目要求"));
    assert!(error.contains("不在 CLI 支持范围"));
}

#[test]
fn missing_legacy_fields_stay_missing_and_diagnostic_explains_upgrade() -> TestResult {
    let mut legacy: ProjectConfig = toml::from_str("name='legacy'\nversion='1.0.0'")?;
    assert_eq!(legacy.min_cli_version, None);
    assert_eq!(legacy.format_version, None);
    assert!(!toml::to_string(&legacy)?.contains("min_cli_version"));
    for missing in ["both", "minimum", "format"] {
        legacy.min_cli_version = (missing == "format").then(|| "0.2.1".into());
        legacy.format_version = (missing == "minimum").then(|| "0.2.0".into());
        let error = check(&legacy, "0.2.1")
            .expect_err("旧配置不能自动补齐")
            .to_string();
        for required in [
            "实际 Kenvoy CLI：0.2.1",
            "最低 CLI：",
            "项目格式：",
            ">=0.2.0, <0.4.0",
            "旧配置升级路径",
            "migrate baseline",
            "migrate history",
        ] {
            assert!(error.contains(required), "{required}: {error}");
        }
    }
    Ok(())
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-compatibility-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
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
fn load_keeps_legacy_history_accessible_but_new_templates_are_explicit() -> TestResult {
    let scratch = Scratch::new()?;
    fs::write(
        scratch.0.join("kenvoy.toml"),
        "[project]\nversion='1.0.0'\n",
    )?;
    let old = Project::load(&scratch.0)?;
    assert_eq!(old.config.project.min_cli_version, None);
    assert_eq!(old.config.project.format_version, None);
    assert!(old.check_compatibility().is_err());
    let fresh = scratch.0.join("fresh");
    super::create(&fresh)?;
    let new = Project::load(&fresh)?;
    assert_eq!(
        new.config.project.min_cli_version.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(new.config.project.format_version.as_deref(), Some("0.3.0"));
    new.check_compatibility()?;
    Ok(())
}
