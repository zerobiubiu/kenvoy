//! 同 Schema 数据步骤与分步事务恢复，均由正式 CLI 生成、固化和执行。

use std::{collections::BTreeMap, time::Duration};

use kenvoy_tests::TestResult;
use rusqlite::Connection;
use serde_json::json;

use super::support::{
    apply, command, data_rules, db, history, insert_customer, migration, project, reject, rules,
    scalar, seal,
};

#[test]
fn data_only_migration_changes_chain_position_and_runs_exactly_once() -> TestResult {
    let fixture = project("data-only-migration")?;
    apply(&fixture)?;
    insert_customer(&fixture, "数据迁移", 40)?;
    let before = migration(&fixture, &["status"])?;
    let path = data_rules(
        &fixture,
        "increment-age",
        "UPDATE customers SET age=age+1 WHERE name='数据迁移'",
    )?;
    let sealed = seal(&fixture, "保持Schema的年龄数据修正", Some(&path))?;
    assert_eq!(
        sealed["position"]["schema_hash"],
        before["position"]["schema_hash"]
    );
    assert_ne!(
        sealed["position"]["checksum"],
        before["position"]["checksum"]
    );
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 40);
    let pending = migration(&fixture, &["status"])?;
    assert_eq!(
        pending["pending"].as_array().ok_or("缺少待执行步骤")?.len(),
        1
    );

    reject(&command(&fixture, &["run"])?)?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 40);
    assert_eq!(history(&fixture)?.len(), 1);
    let checksum = pending["confirmation_checksum"]
        .as_str()
        .ok_or("数据迁移缺少执行确认值")?;
    let mut confirmed = fixture.start_with(
        "run",
        &["--confirm", checksum],
        Duration::from_secs(45),
        &BTreeMap::new(),
    )?;
    confirmed.stop()?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 41);
    let current = migration(&fixture, &["status"])?;
    assert_eq!(current["position"], sealed["position"]);
    assert_eq!(current["status"], "current");
    assert_eq!(history(&fixture)?.len(), 2);
    apply(&fixture)?;
    let mut server = fixture.start("run")?;
    server.stop()?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 41);
    assert_eq!(history(&fixture)?.len(), 2, "重复启动不得重放数据迁移");
    Ok(())
}

#[test]
fn failed_step_rolls_back_but_keeps_committed_prefix_and_can_resume() -> TestResult {
    let fixture = project("migration-resume")?;
    apply(&fixture)?;
    insert_customer(&fixture, "保留前序提交", 20)?;
    insert_customer(&fixture, "阻断", 5)?;
    let first_rules = data_rules(&fixture, "first-step", "UPDATE customers SET age=age+1")?;
    let first = seal(&fixture, "可独立提交的前序数据修正", Some(&first_rules))?;
    let failure_rules = rules(
        &fixture,
        "conditional-failure",
        &json!({
            "format":1,
            "after_sql":["UPDATE customers SET age=age+100"],
            "assertions":[{
                "sql":"SELECT COUNT(*) FROM customers WHERE name='阻断'",
                "expected":0,
                "description":"存在阻断数据时不得提交当前步骤"
            }]
        }),
    )?;
    let last = seal(&fixture, "真实数据上的条件断言", Some(&failure_rules))?;
    let pending = migration(&fixture, &["status"])?;
    let checksum = pending["confirmation_checksum"]
        .as_str()
        .ok_or("缺少执行确认值")?;
    let error = reject(&command(
        &fixture,
        &["migrate", "apply", "--confirm", checksum],
    )?)?;
    assert!(error.contains("断言") || error.contains("阻断"), "{error}");
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT age FROM customers WHERE name='保留前序提交'"
        )?,
        21
    );
    assert_eq!(
        scalar(&db(&fixture), "SELECT age FROM customers WHERE name='阻断'")?,
        6
    );
    assert_eq!(history(&fixture)?.len(), 2, "失败步骤不能伪装为已应用");
    let partial = migration(&fixture, &["status"])?;
    assert_eq!(partial["position"], first["position"]);
    assert_eq!(
        partial["pending"].as_array().ok_or("缺少待恢复步骤")?.len(),
        1
    );

    // 只修复本测试的数据前置条件，不修改已固化的迁移或 checksum。
    Connection::open(db(&fixture))?
        .execute("UPDATE customers SET name='已修复' WHERE name='阻断'", [])?;
    apply(&fixture)?;
    assert_eq!(
        migration(&fixture, &["status"])?["position"],
        last["position"]
    );
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT age FROM customers WHERE name='保留前序提交'"
        )?,
        121
    );
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT age FROM customers WHERE name='已修复'"
        )?,
        106
    );
    assert_eq!(history(&fixture)?.len(), 3);
    apply(&fixture)?;
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT age FROM customers WHERE name='保留前序提交'"
        )?,
        121
    );
    Ok(())
}
