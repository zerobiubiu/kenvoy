//! 运行入口消费正式链、启动审计、进程互斥与无源码交付。

use std::{collections::BTreeMap, fs, net::TcpListener, path::Path, time::Duration};

use kenvoy_tests::{Fixture, TestResult, require_success};
use reqwest::Client;
use serde_json::Value;

use super::support::{
    append_fields, apply, command, db, history, insert_customer, migration, project, reject,
    scalar, seal,
};

fn startups(fixture: &Fixture) -> TestResult<Vec<Value>> {
    let path = db(fixture).with_file_name("app.sqlite.kenvoy-startups.jsonl");
    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

async fn application(client: &Client, origin: &str) -> TestResult<Value> {
    let response = client
        .get(format!("{origin}/_kenvoy/application"))
        .send()
        .await?;
    assert!(response.status().is_success());
    Ok(serde_json::from_slice(&response.bytes().await?)?)
}

#[tokio::test]
async fn empty_database_starts_from_chain_and_restart_does_not_replay() -> TestResult {
    let fixture = project("catalog-startup")?;
    assert!(!db(&fixture).exists());
    let pending = migration(&fixture, &["status"])?;
    assert_eq!(pending["pending"].as_array().ok_or("缺少初始链")?.len(), 1);
    assert!(!db(&fixture).exists(), "只读status不能创建数据库");
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let mut server = fixture.start("run")?;
    let model = application(&client, server.origin()).await?;
    assert_eq!(model["resources"][0]["name"], "customers");
    server.stop()?;
    insert_customer(&fixture, "重启后仍存在", 28)?;
    let current = migration(&fixture, &["status"])?;
    let applied = history(&fixture)?;
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["id"], current["position"]["id"]);
    let mut second = fixture.start("run")?;
    application(&client, second.origin()).await?;
    second.stop()?;
    assert_eq!(history(&fixture)?, applied);
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 28);
    let attempts = startups(&fixture)?;
    let ready: Vec<_> = attempts
        .iter()
        .filter(|entry| entry["phase"] == "ready")
        .collect();
    assert_eq!(ready.len(), 2);
    assert_ne!(ready[0]["startup_id"], ready[1]["startup_id"]);
    assert_eq!(
        ready[1]["database_migration"]["position"],
        current["position"]
    );
    Ok(())
}

#[test]
fn minimum_cli_and_unknown_project_format_are_rejected_before_any_write() -> TestResult {
    let fixture = project("cli-compatibility")?;
    let config = fixture.project().join("kenvoy.toml");
    let original = fs::read_to_string(&config)?;
    for field in ["min_cli_version", "format_version"] {
        let prefix = format!("{field} = ");
        let needle = original
            .lines()
            .find(|line| line.starts_with(&prefix))
            .ok_or_else(|| format!("缺少兼容声明：{field}"))?;
        fs::write(
            &config,
            original.replace(needle, &format!("{field} = \"999.0.0\"")),
        )?;
        for args in [
            vec!["check"],
            vec!["inspect"],
            vec!["build"],
            vec!["run"],
            vec!["dev"],
            vec!["migrate", "status"],
        ] {
            let error = reject(&command(&fixture, &args)?)?;
            assert!(error.contains(field), "必须指出不兼容的声明：{error}");
            assert!(!db(&fixture).exists());
            assert!(!fixture.project().join("dist").exists());
        }
        let history = migration(&fixture, &["history"])?;
        assert_eq!(history["applied"], serde_json::json!([]));
        assert_eq!(history["legacy_history"], serde_json::json!([]));
    }
    fs::write(
        &config,
        original
            .lines()
            .filter(|line| {
                !line.starts_with("min_cli_version") && !line.starts_with("format_version")
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    reject(&command(&fixture, &["check"])?)?;
    assert!(!db(&fixture).exists(), "缺失兼容声明不能被猜成当前格式");
    Ok(())
}

#[test]
fn changed_schema_requires_sealed_migration_instead_of_runtime_diff() -> TestResult {
    let fixture = project("unsealed-schema")?;
    apply(&fixture)?;
    insert_customer(&fixture, "正式链保护", 31)?;
    append_fields(&fixture, "<text name='unsealed' />")?;
    reject(&command(&fixture, &["run"])?)?;
    reject(&command(&fixture, &["dev"])?)?;
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='unsealed'"
        )?,
        0
    );
    assert_eq!(history(&fixture)?.len(), 1);
    seal(&fixture, "显式发布新增字段", None)?;
    let mut server = fixture.start("run")?;
    server.stop()?;
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='unsealed'"
        )?,
        1
    );
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 31);
    assert_eq!(history(&fixture)?.len(), 2);
    Ok(())
}

#[test]
fn committed_migration_and_failed_listener_are_distinct_startup_facts() -> TestResult {
    let fixture = project("startup-after-migrate")?;
    let occupied = TcpListener::bind("127.0.0.1:0")?;
    let config_path = fixture.project().join("kenvoy.toml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace(
            "port = 0",
            &format!("port = {}", occupied.local_addr()?.port()),
        ),
    )?;
    let output = fixture.command(&["--json-logs", "--project", "app", "run"], "")?;
    let error = reject(&output)?;
    assert!(error.contains("数据库已迁移，应用未就绪"), "{error}");
    let status = migration(&fixture, &["status"])?;
    assert_eq!(status["status"], "current");
    assert_eq!(history(&fixture)?.len(), 1);
    let attempts = startups(&fixture)?;
    let migrated = attempts
        .iter()
        .find(|entry| entry["phase"] == "migrated")
        .ok_or("缺少迁移完成审计")?;
    let failed = attempts
        .iter()
        .find(|entry| entry["phase"] == "startup_failed")
        .ok_or("缺少启动失败审计")?;
    assert_eq!(failed["startup_id"], migrated["startup_id"]);
    assert_eq!(failed["database_migration"]["position"], status["position"]);
    assert!(
        failed["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty())
    );
    assert!(!attempts.iter().any(|entry| entry["phase"] == "ready"));
    assert!(
        failed["project_version"].is_string()
            && failed["build_id"].is_string()
            && failed["cli_version"].is_string()
    );
    drop(occupied);
    fs::write(config_path, config)?;
    let mut server = fixture.start("run")?;
    server.stop()?;
    assert_eq!(history(&fixture)?.len(), 1);
    let attempts = startups(&fixture)?;
    let ready = attempts
        .iter()
        .find(|entry| entry["phase"] == "ready")
        .ok_or("重试缺少ready审计")?;
    assert_ne!(ready["startup_id"], failed["startup_id"]);
    Ok(())
}

#[tokio::test]
async fn second_process_cannot_use_database_until_first_server_exits() -> TestResult {
    let fixture = project("exclusive-database")?;
    let mut first = fixture.start("run")?;
    insert_customer(&fixture, "单一运行实例", 52)?;
    let args = ["--project", "app", "run"];
    let output = fixture.command_with(&args, "", Duration::from_secs(10))?;
    let error = reject(&output)?;
    assert!(error.contains("锁") || error.contains("占用"), "{error}");
    let output = fixture.command_with(
        &["--project", "app", "migrate", "apply"],
        "",
        Duration::from_secs(10),
    )?;
    reject(&output)?;
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    application(&client, first.origin()).await?;
    assert_eq!(
        scalar(&db(&fixture), "SELECT COUNT(*) FROM _kenvoy_migrations")?,
        1
    );
    first.stop()?;
    let mut replacement = fixture.start("run")?;
    application(&client, replacement.origin()).await?;
    replacement.stop()?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 52);
    Ok(())
}

#[tokio::test]
async fn source_free_artifact_carries_full_chain_for_a_fresh_database() -> TestResult {
    let fixture = project("source-free-chain")?;
    append_fields(&fixture, "<text name='artifact_note' />")?;
    seal(&fixture, "交付产物中的后续Schema", None)?;
    require_success(&command(&fixture, &["build"])?)?;
    assert!(!db(&fixture).exists(), "构建完整链不应创建数据库");
    for name in ["src", "migrations", ".kenvoy"] {
        let path = fixture.project().join(name);
        if path.exists() {
            fs::rename(path, fixture.root().join(format!("archive-{name}")))?;
        }
    }
    assert!(!fixture.project().join("src").exists());
    assert!(!fixture.project().join("migrations").exists());
    let path = Path::new(fixture.root())
        .to_str()
        .ok_or("临时路径不是UTF-8")?;
    let env = BTreeMap::from([("PATH".into(), path.into()), ("Path".into(), path.into())]);
    let mut server = fixture.start_with("run", &[], Duration::from_secs(45), &env)?;
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let model = application(&client, server.origin()).await?;
    assert!(
        model["resources"][0]["fields"]
            .as_array()
            .ok_or("缺少字段投影")?
            .iter()
            .any(|field| field["name"] == "artifact_note")
    );
    server.stop()?;
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='artifact_note'"
        )?,
        1
    );
    assert_eq!(history(&fixture)?.len(), 2);
    let current = migration(&fixture, &["status"])?;
    assert_eq!(current["status"], "current");
    apply(&fixture)?;
    assert_eq!(history(&fixture)?.len(), 2);
    Ok(())
}
