//! 正式字段改名必须保留附件绑定；本步失败后仍可从真实位置恢复。
use super::*;

#[tokio::test]
async fn file_rename_keeps_blob_and_binding_atomic_across_failure_and_resume() -> TestResult {
    let fixture = Fixture::new()?;
    let initial = fixture.application(ATTACHMENTS)?;
    migration::catalog::generate(&fixture.root, &initial, "初始附件结构", None)?;
    migration::catalog::seal(&fixture.root, &initial, None)?;
    let before = migration::catalog::read(&fixture.root)?;
    let database = Database::open_catalog(
        fixture.database.clone(),
        initial.clone(),
        before.clone(),
        None,
    )
    .await?;
    let upload = database
        .stage_upload(attachment('a', "attachments", "改名保留.pdf"))
        .await?;
    let record = database
        .create(
            "customers",
            json!({"name":"阻断", "attachments":[upload]}),
            Some("a".repeat(64)),
        )
        .await?;
    let id = record["id"].as_i64().ok_or("缺少记录 ID")?;
    database.shutdown().await?;

    let target = fixture.application("<text name='name' required unique/><file name='documents' maxFiles={2} accept={['application/pdf']}/><file name='secondary' maxFiles={2}/>")?;
    let rules = fixture.root.join("rename.json");
    fs::write(
        &rules,
        serde_json::to_vec(&json!({
            "format":1,"renames":[{"table":"customers","from":"attachments","to":"documents"}],
            "after_sql":["UPDATE customers SET documents='[]' WHERE name='阻断'"]
        }))?,
    )?;
    let draft =
        migration::catalog::generate(&fixture.root, &target, "保留附件的显式改名", Some(&rules))?;
    migration::catalog::seal(
        &fixture.root,
        &target,
        draft["confirmation_checksum"].as_str(),
    )?;
    let after = migration::catalog::read(&fixture.root)?;
    let confirmation =
        migration::catalog::status(&fixture.database, &after)?["confirmation_checksum"]
            .as_str()
            .ok_or("缺少确认值")?
            .to_owned();
    assert!(
        Database::open_catalog(
            fixture.database.clone(),
            target.clone(),
            after.clone(),
            Some(confirmation)
        )
        .await
        .is_err()
    );
    assert_eq!(
        migration::catalog::history(&fixture.database)?["applied"]
            .as_array()
            .ok_or("缺少真实执行历史")?
            .len(),
        1
    );

    let old = Database::open_catalog(fixture.database.clone(), initial, before, None).await?;
    assert_eq!(
        old.get("customers", id).await?.ok_or("旧记录丢失")?["attachments"],
        record["attachments"]
    );
    assert_eq!(
        old.file_info(&upload.id)
            .await?
            .ok_or("回滚后附件丢失")?
            .field,
        "attachments"
    );
    assert_eq!(
        old.file_content(&upload.id).await?,
        Some(b"%PDF-1.7\nbinary".to_vec())
    );
    old.update("customers", id, json!({"name":"前提已修复"}), None)
        .await?;
    old.shutdown().await?;

    let confirmation =
        migration::catalog::status(&fixture.database, &after)?["confirmation_checksum"]
            .as_str()
            .ok_or("缺少恢复确认值")?
            .to_owned();
    let migrated =
        Database::open_catalog(fixture.database.clone(), target, after, Some(confirmation)).await?;
    let value = migrated
        .get("customers", id)
        .await?
        .ok_or("迁移后记录丢失")?;
    assert_eq!(value["documents"], record["attachments"]);
    assert!(!value.contains_key("attachments"));
    assert_eq!(
        migrated
            .file_info(&upload.id)
            .await?
            .ok_or("迁移后附件丢失")?
            .field,
        "documents"
    );
    assert_eq!(
        migrated.file_content(&upload.id).await?,
        Some(b"%PDF-1.7\nbinary".to_vec())
    );
    migrated.shutdown().await?;
    Ok(())
}
