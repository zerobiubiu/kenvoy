//! 旧库显式基线接入与开发快照不能相互冒充正式迁移。

use std::{collections::BTreeMap, fs, time::Duration};

use kenvoy_tests::TestResult;
use rusqlite::Connection;
use serde_json::json;

use super::support::{
    append_fields, apply, command, data_rules, db, history, insert_customer, migration, project,
    read_db, reject, rules, scalar, seal,
};

#[test]
fn legacy_baseline_requires_proof_and_confirmation_without_inventing_execution() -> TestResult {
    let fixture = project("legacy-baseline")?;
    apply(&fixture)?;
    insert_customer(&fixture, "已执行的旧数据迁移", 40)?;
    let path = data_rules(
        &fixture,
        "legacy-data-step",
        "UPDATE customers SET age=age+1",
    )?;
    let sealed = seal(&fixture, "需以数据证据确认的旧数据步骤", Some(&path))?;
    apply(&fixture)?;
    let id = sealed["position"]["id"].as_str().ok_or("固化步骤缺少ID")?;
    let old = migration(&fixture, &["history"])?["legacy_history"].clone();
    assert_eq!(old.as_array().ok_or("缺少旧历史")?.len(), 2);
    // 仅移除本测试拥有的正式链标记，保留真实已执行的旧Schema与审计事实。
    Connection::open(db(&fixture))?.execute_batch(
        "DROP TABLE _kenvoy_catalog_applied; DROP TABLE _kenvoy_catalog_baseline; DROP TABLE _kenvoy_catalog_owner;",
    )?;
    let status = migration(&fixture, &["status"])?;
    assert_eq!(status["status"], "baseline_required");
    reject(&command(&fixture, &["migrate", "apply"])?)?;
    reject(&command(&fixture, &["run"])?)?;
    reject(&command(&fixture, &["migrate", "baseline", "--at", id])?)?;
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM sqlite_master WHERE name='_kenvoy_catalog_owner'"
        )?,
        0
    );

    let evidence = rules(
        &fixture,
        "baseline-evidence",
        &json!({
            "format":1,
            "baseline_proofs":[{
                "step":id,
                "assertions":[{
                    "sql":"SELECT COUNT(*) FROM customers WHERE age<>41",
                    "expected":0,
                    "description":"旧库已经完成指定的年龄数据修正"
                }]
            }]
        }),
    )?;
    let evidence = evidence.to_str().ok_or("证据路径不是UTF-8")?;
    let preview = migration(&fixture, &["baseline", "--at", id, "--rules", evidence])?;
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["target_baseline"], sealed["position"]);
    assert_eq!(preview["legacy_history"], old);
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM sqlite_master WHERE name='_kenvoy_catalog_owner'"
        )?,
        0,
        "预览不得创建基线元数据"
    );
    let checksum = preview["confirmation_checksum"]
        .as_str()
        .ok_or("基线预览缺少确认值")?;
    reject(&command(
        &fixture,
        &[
            "migrate",
            "baseline",
            "--at",
            id,
            "--rules",
            evidence,
            "--confirm",
            "过期基线",
        ],
    )?)?;
    migration(
        &fixture,
        &[
            "baseline",
            "--at",
            id,
            "--rules",
            evidence,
            "--confirm",
            checksum,
        ],
    )?;
    let status = migration(&fixture, &["status"])?;
    assert_eq!(status["status"], "current");
    assert_eq!(status["position"], sealed["position"]);
    assert!(status["baseline"].is_object());
    assert_eq!(status["baseline"]["at_id"], id);
    assert_eq!(
        status["baseline"]["at_checksum"],
        sealed["position"]["checksum"]
    );
    assert_eq!(status["baseline"]["legacy_history"], old);
    assert!(
        history(&fixture)?.is_empty(),
        "基线不能伪造先前正式步骤的已执行记录"
    );
    let audit = migration(&fixture, &["history"])?;
    assert_eq!(audit["legacy_history"], old, "不得覆盖或追加虚假的旧历史");
    assert!(audit["baseline"].is_object());
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 41);
    apply(&fixture)?;
    let mut server = fixture.start("run")?;
    server.stop()?;
    assert_eq!(migration(&fixture, &["history"])?["legacy_history"], old);
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 41);
    Ok(())
}

#[test]
fn explicit_dev_snapshot_isolated_from_formal_data_and_cannot_be_promoted() -> TestResult {
    let fixture = project("snapshot-isolation")?;
    apply(&fixture)?;
    insert_customer(&fixture, "正式数据", 33)?;
    let before = migration(&fixture, &["history"])?;
    append_fields(&fixture, "<text name='snapshot_only' />")?;
    let snapshot = fixture.project().join("dev-snapshots/preview.sqlite");
    let mut server = fixture.start_with(
        "dev",
        &["--snapshot", "dev-snapshots/preview.sqlite"],
        Duration::from_secs(45),
        &BTreeMap::new(),
    )?;
    assert_eq!(
        scalar(
            &snapshot,
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='snapshot_only'"
        )?,
        1
    );
    assert_eq!(
        scalar(&snapshot, "SELECT COUNT(*) FROM customers")?,
        0,
        "一次性空快照不复制正式业务数据"
    );
    Connection::open(&snapshot)?.execute(
        "INSERT INTO customers(name,phone,age) VALUES('快照记录','仅快照',99)",
        [],
    )?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 33);
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='snapshot_only'"
        )?,
        0
    );
    server.stop()?;
    assert_eq!(migration(&fixture, &["history"])?, before);
    let marker = snapshot.with_file_name("preview.sqlite.kenvoy-snapshot.json");
    let marker: serde_json::Value = serde_json::from_slice(&fs::read(marker)?)?;
    assert_eq!(marker["format_version"], 1);
    assert_eq!(marker["kind"], "development_snapshot");
    assert!(marker["schema_hash"].is_string());

    let sealed = seal(&fixture, "正式库尚未执行的新增字段", None)?;
    let id = sealed["position"]["id"].as_str().ok_or("固化步骤缺少ID")?;
    let config_path = fixture.project().join("kenvoy.toml");
    let config = fs::read_to_string(&config_path)?;
    let needle = "path = \"data/app.sqlite\"";
    assert!(config.contains(needle));
    fs::write(
        &config_path,
        config.replace(needle, "path = \"dev-snapshots/preview.sqlite\""),
    )?;
    for args in [
        vec!["run"],
        vec!["migrate", "apply"],
        vec!["migrate", "baseline", "--at", id],
    ] {
        let error = reject(&command(&fixture, &args)?)?;
        assert!(
            error.contains("快照"),
            "必须明确拒绝开发快照冒充正式库：{error}"
        );
    }
    fs::write(config_path, config)?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 33);
    let connection = read_db(&db(&fixture))?;
    let phone: String =
        connection.query_row("SELECT phone FROM customers", [], |row| row.get(0))?;
    assert_eq!(phone, "保留电话");
    assert_eq!(migration(&fixture, &["history"])?, before);
    Ok(())
}
