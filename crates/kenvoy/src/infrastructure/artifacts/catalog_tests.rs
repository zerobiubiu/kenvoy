use super::{
    ArtifactError, Loaded, Manifest, Staging, activate, files, input_fingerprint, load,
    tests::{compile, freeze_catalog},
};
use crate::{
    domain::model::ApplicationIR,
    infrastructure::{
        persistence::sqlite::migration::catalog,
        project::{self, Project},
    },
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Fixture(PathBuf);
impl Fixture {
    fn new(with_catalog: bool) -> TestResult<(Self, Project, ApplicationIR)> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-artifact-catalog-{}-{stamp}",
            std::process::id()
        ));
        let fixture = Self(path);
        project::create(&fixture.0)?;
        let project = Project::load(&fixture.0)?;
        let (application, _) = compile(&project, &mut kengine::AnalysisSession::new())?;
        if with_catalog {
            freeze_catalog(&project, &application)?;
        }
        Ok((fixture, project, application))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn build(project: &Project, application: &ApplicationIR) -> TestResult<Loaded> {
    let stage = Staging::new(project)?;
    crate::infrastructure::frontend::export(&stage.client())?;
    let output = stage.seal(
        application,
        b"{}",
        input_fingerprint(project)?,
        BTreeMap::new(),
    )?;
    activate(project, &output.manifest)?;
    Ok(output)
}

// 重新计算外层全部摘要，使测试真正触达迁移链和目标模型的内部校验。
fn resign(project: &Project, previous: &Manifest, mut manifest: Manifest) -> TestResult<Manifest> {
    let old = project.out_dir.join("versions").join(&previous.build_id);
    manifest.files = files::collect(&old)?;
    manifest.build_id = manifest.identity()?;
    let new = project.out_dir.join("versions").join(&manifest.build_id);
    if old != new {
        fs::rename(old, new)?;
    }
    fs::write(
        project.out_dir.join("manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(manifest)
}

fn load_error(project: &Project) -> String {
    match load(project) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("篡改产物不应加载成功"),
    }
}

#[test]
fn artifact_migrations_execute_without_source_or_project_migration_files() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    let expected = serde_json::to_value(catalog::read(&project.root)?)?;
    let output = build(&project, &application)?;
    assert_eq!(output.manifest.format_version, 2);
    assert!(output.manifest.files.contains_key("migrations.json"));
    assert!(
        output
            .manifest
            .files
            .keys()
            .all(|name| !name.ends_with(".tsx") && !name.starts_with("data/"))
    );
    fs::remove_dir_all(project.root.join("src"))?;
    fs::remove_dir_all(project.root.join("migrations"))?;
    let loaded = load(&project)?;
    assert_eq!(serde_json::to_value(&loaded.migrations)?, expected);
    let mut connection = rusqlite::Connection::open_in_memory()?;
    catalog::apply(
        &mut connection,
        Path::new(":memory:"),
        &loaded.migrations,
        None,
    )?;
    assert_eq!(
        connection.execute("INSERT INTO customers(name) VALUES(?1)", ["独立交付"])?,
        1
    );
    let count: i64 =
        connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))?;
    assert_eq!(count, 1);
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn migration_sql_tampering_is_rejected_even_after_resigning_outer_assets() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    let output = build(&project, &application)?;
    let mut changed = output.migrations.clone();
    changed.steps[0]
        .rules
        .after_sql
        .push("DELETE FROM customers".into());
    let version = project
        .out_dir
        .join("versions")
        .join(&output.manifest.build_id);
    fs::write(
        version.join("migrations.json"),
        serde_json::to_vec(&changed)?,
    )?;
    resign(&project, &output.manifest, output.manifest.clone())?;
    assert!(matches!(load(&project), Err(ArtifactError::Migration(_))));
    assert!(!project.database.exists());
    Ok(())
}

#[test]
fn catalog_target_must_match_artifact_ir_after_outer_assets_are_resigned() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    let output = build(&project, &application)?;
    let mut changed = output.application.clone();
    changed.resources[0].fields[0].name = "renamed".into();
    let version = project
        .out_dir
        .join("versions")
        .join(&output.manifest.build_id);
    fs::write(
        version.join("application.ir"),
        serde_json::to_vec(&changed)?,
    )?;
    resign(&project, &output.manifest, output.manifest.clone())?;
    assert!(matches!(load(&project), Err(ArtifactError::Migration(_))));
    Ok(())
}

#[test]
fn old_artifact_format_is_rejected_with_and_without_v2_fields() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    let output = build(&project, &application)?;
    let mut old = output.manifest.clone();
    old.format_version = 1;
    let old = resign(&project, &output.manifest, old)?;
    assert!(load_error(&project).contains("只接受 v2"));
    let mut legacy = serde_json::to_value(old)?;
    for field in [
        "project_name",
        "project_version",
        "min_cli_version",
        "project_format_version",
        "development_snapshot",
        "input_hash_version",
        "build_hash_version",
        "schema_hash_version",
    ] {
        legacy.as_object_mut().expect("真实清单对象").remove(field);
    }
    fs::write(
        project.out_dir.join("manifest.json"),
        serde_json::to_vec(&legacy)?,
    )?;
    assert!(load(&project).is_err());
    Ok(())
}

#[test]
fn algorithm_versions_and_framework_identity_have_independent_gates() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    let output = build(&project, &application)?;
    for field in [
        "input_hash_version",
        "build_hash_version",
        "schema_hash_version",
        "framework_version",
        "framework_fingerprint",
    ] {
        let mut value = serde_json::to_value(&output.manifest)?;
        value[field] = "不受支持的版本".into();
        let mut changed: Manifest = serde_json::from_value(value)?;
        changed.build_id = changed.identity()?;
        assert_ne!(
            changed.build_id, output.manifest.build_id,
            "{field} 必须纳入构建标识"
        );
        let error =
            match super::load::load_manifest(&project.root, &project.out_dir, changed, false) {
                Err(error) => error.to_string(),
                Ok(_) => panic!("{field} 门禁不应放行"),
            };
        assert!(
            error.contains(if field.starts_with("framework") {
                "框架"
            } else {
                "算法版本"
            }),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn source_free_config_cannot_impersonate_another_project_version() -> TestResult {
    let (_fixture, project, application) = Fixture::new(true)?;
    build(&project, &application)?;
    fs::remove_dir_all(project.root.join("src"))?;
    for field in ["name", "version", "min_cli_version", "format_version"] {
        let mut changed = project.config.clone();
        match field {
            "name" => changed.project.name = "另一个应用".into(),
            "version" => changed.project.version = "1.2.3".into(),
            "min_cli_version" => changed.project.min_cli_version = Some("0.2.0".into()),
            "format_version" => changed.project.format_version = Some("0.2.1".into()),
            _ => unreachable!(),
        }
        fs::write(project.root.join("kenvoy.toml"), toml::to_string(&changed)?)?;
        let reloaded = Project::load(&project.root)?;
        reloaded.check_compatibility()?;
        assert!(
            load_error(&reloaded).contains("与 kenvoy.toml 不一致"),
            "{field}"
        );
    }
    Ok(())
}

#[test]
fn formal_build_does_not_create_migrations_and_snapshot_cannot_be_run_formally() -> TestResult {
    let (_fixture, mut project, application) = Fixture::new(false)?;
    let formal_input = input_fingerprint(&project)?;
    assert!(build(&project, &application).is_err());
    assert!(!project.root.join("migrations").exists());
    assert!(!project.database.exists());
    project.use_snapshot(Path::new("dev-snapshots/try.sqlite"))?;
    assert_ne!(input_fingerprint(&project)?, formal_input);
    let snapshot_input = input_fingerprint(&project)?;
    fs::write(
        project.root.join("data/credentials.json"),
        "正式数据目录密钥夹具",
    )?;
    assert_eq!(snapshot_input, input_fingerprint(&project)?);
    let snapshot = build(&project, &application)?;
    assert!(snapshot.manifest.development_snapshot);
    assert!(snapshot.migrations.steps.is_empty());
    project.snapshot = false;
    assert!(load_error(&project).contains("开发快照产物不能用于正式运行"));
    Ok(())
}

#[test]
fn fingerprints_cover_all_migration_files_but_not_data_secrets_or_timestamps() -> TestResult {
    let (_fixture, project, _) = Fixture::new(true)?;
    fs::create_dir_all(project.root.join("migrations/rules"))?;
    for name in [
        "manifest.json",
        "rules/fill.toml",
        "rules/data.sql",
        "draft.arbitrary",
    ] {
        let path = project.root.join("migrations").join(name);
        fs::write(&path, "变更前")?;
        let before = input_fingerprint(&project)?;
        fs::write(&path, "变更后")?;
        let after = input_fingerprint(&project)?;
        assert_ne!(before, after, "{name}");
        fs::write(&path, "变更后")?;
        assert_eq!(after, input_fingerprint(&project)?, "时间戳不应进入指纹");
    }
    let before = input_fingerprint(&project)?;
    for (name, content) in [
        ("data/app.sqlite", "自有数据夹具"),
        ("data/credentials.json", "数据目录中的私有配置"),
        (".env", "SECRET=不可入产物"),
        (".env.production", "SECRET=生产夹具"),
        ("src/.env.local", "SECRET=源码目录夹具"),
    ] {
        fs::write(project.root.join(name), content)?;
        assert_eq!(before, input_fingerprint(&project)?, "{name}");
    }
    Ok(())
}
