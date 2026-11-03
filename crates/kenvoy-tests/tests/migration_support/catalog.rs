//! 从真实迁移文件破坏/替换交付链，断言消费者在写库前拒绝。

use std::fs;

use kenvoy_tests::TestResult;
use serde_json::Value;

use super::support::{
    append_fields, apply, command, copy_owned_directory, db, insert_customer, manifest,
    manifest_path, migration, project, reject, scalar, seal, source, step_file,
};

#[test]
fn rewritten_migration_is_rejected_before_any_database_change() -> TestResult {
    let fixture = project("chain-rewrite")?;
    apply(&fixture)?;
    insert_customer(&fixture, "不可改写历史", 20)?;
    let path = step_file(&fixture, 0)?;
    let mut step: Value = serde_json::from_slice(&fs::read(&path)?)?;
    step["description"] = "被改写的历史".into();
    fs::write(&path, serde_json::to_vec_pretty(&step)?)?;

    let error = reject(&command(&fixture, &["migrate", "status"])?)?;
    assert!(
        error.contains("校验") || error.contains("checksum"),
        "{error}"
    );
    reject(&command(&fixture, &["migrate", "apply"])?)?;
    reject(&command(&fixture, &["run"])?)?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 20);
    assert_eq!(
        scalar(&db(&fixture), "SELECT COUNT(*) FROM _kenvoy_migrations")?,
        1
    );
    Ok(())
}

#[test]
fn missing_step_and_duplicate_manifest_entries_are_not_empty_history() -> TestResult {
    let fixture = project("chain-missing")?;
    apply(&fixture)?;
    let path = step_file(&fixture, 0)?;
    let bytes = fs::read(&path)?;
    fs::remove_file(&path)?;
    reject(&command(&fixture, &["migrate", "status"])?)?;
    reject(&command(&fixture, &["migrate", "apply"])?)?;
    assert_eq!(
        scalar(&db(&fixture), "SELECT COUNT(*) FROM _kenvoy_migrations")?,
        1
    );

    fs::write(path, bytes)?;
    let mut chain = manifest(&fixture)?;
    let entries = chain["steps"].as_array_mut().ok_or("缺少正式链")?;
    entries.push(entries.first().ok_or("正式链为空")?.clone());
    fs::write(manifest_path(&fixture), serde_json::to_vec_pretty(&chain)?)?;
    reject(&command(&fixture, &["migrate", "status"])?)?;
    reject(&command(&fixture, &["run"])?)?;
    assert_eq!(
        scalar(&db(&fixture), "SELECT COUNT(*) FROM _kenvoy_migrations")?,
        1
    );
    Ok(())
}

#[test]
fn independently_valid_fork_cannot_replace_an_applied_prefix() -> TestResult {
    let fixture = project("chain-fork")?;
    apply(&fixture)?;
    insert_customer(&fixture, "分叉前数据", 37)?;
    let initial_chain = fixture.root().join("initial-chain");
    copy_owned_directory(&fixture.project().join("migrations"), &initial_chain)?;
    let original = append_fields(&fixture, "<text name='branch_a' />")?;
    let first = seal(&fixture, "历史分支甲", None)?;
    apply(&fixture)?;
    assert_eq!(migration(&fixture, &["status"])?["status"], "current");

    // 两条链都由 CLI 生成并固化；不是仅靠损坏 JSON 或旧 checksum 触发拒绝。
    fs::remove_dir_all(fixture.project().join("migrations"))?;
    copy_owned_directory(&initial_chain, &fixture.project().join("migrations"))?;
    fs::write(source(&fixture), original)?;
    append_fields(&fixture, "<text name='branch_b' />")?;
    let second = seal(&fixture, "历史分支乙", None)?;
    assert_eq!(first["position"]["id"], second["position"]["id"]);
    assert_ne!(
        first["position"]["checksum"],
        second["position"]["checksum"]
    );
    reject(&command(&fixture, &["migrate", "status"])?)?;
    reject(&command(&fixture, &["migrate", "apply"])?)?;
    assert_eq!(scalar(&db(&fixture), "SELECT age FROM customers")?, 37);
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='branch_a'"
        )?,
        1
    );
    assert_eq!(
        scalar(
            &db(&fixture),
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='branch_b'"
        )?,
        0
    );
    assert_eq!(
        scalar(&db(&fixture), "SELECT COUNT(*) FROM _kenvoy_migrations")?,
        2
    );
    Ok(())
}
