use super::*;

#[tokio::test]
async fn uploads_survive_restart_and_crud_bind_replace_clear_delete_atomically() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(ATTACHMENTS)?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let first = database
        .stage_upload(attachment('a', "attachments", "一.pdf"))
        .await?;
    let second = database
        .stage_upload(attachment('a', "attachments", "二.pdf"))
        .await?;
    assert_ne!(first.id, second.id);
    let pending = database
        .stage_upload(attachment('a', "attachments", "暂存.pdf"))
        .await?;
    let record = database
        .create(
            "customers",
            json!({"name":"原记录","attachments":[first,second]}),
            Some("a".repeat(64)),
        )
        .await?;
    let id = record["id"].as_i64().ok_or("缺少记录 id")?;
    assert_eq!(
        database
            .file_info(&first.id)
            .await?
            .ok_or("缺少附件")?
            .record_id,
        Some(id)
    );
    assert!(!database.delete_upload(&first.id, &"a".repeat(64)).await?);
    assert!(!database.delete_upload(&pending.id, &"b".repeat(64)).await?);
    let connection = Connection::open(&fixture.database)?;
    let storage: (String, String) = connection.query_row(
        "SELECT typeof(attachments), attachments FROM customers WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(storage.0, "text");
    assert_eq!(
        serde_json::from_str::<Value>(&storage.1)?,
        record["attachments"]
    );
    drop(connection);
    database.shutdown().await?;
    let database = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(database.get("customers", id).await?, Some(record.clone()));
    assert_eq!(
        database.file_content(&first.id).await?,
        Some(b"%PDF-1.7\nbinary".to_vec())
    );
    assert_eq!(
        database
            .file_info(&pending.id)
            .await?
            .ok_or("暂存未持久化")?
            .record_id,
        None
    );
    assert!(database.delete_upload(&pending.id, &"a".repeat(64)).await?);

    let replacement = database
        .stage_upload(attachment('a', "attachments", "替换.pdf"))
        .await?;
    assert!(matches!(
        database
            .create(
                "customers",
                json!({"name":"原记录","attachments":[replacement]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Conflict)
    ));
    assert_eq!(
        database
            .file_info(&replacement.id)
            .await?
            .ok_or("暂存附件消失")?
            .record_id,
        None
    );
    assert!(matches!(
        database
            .update(
                "customers",
                id,
                json!({"name":"不应保存","attachments":[replacement]}),
                Some("b".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(database.get("customers", id).await?, Some(record.clone()));
    let mut forged = serde_json::to_value(&replacement)?;
    forged["size"] = (replacement.size + 1).into();
    assert!(matches!(
        database
            .update(
                "customers",
                id,
                json!({"attachments":[forged]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    let mut missing = serde_json::to_value(&replacement)?;
    missing["id"] = "f".repeat(64).into();
    assert!(matches!(
        database
            .update(
                "customers",
                id,
                json!({"attachments":[replacement],"secondary":[missing]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(database.get("customers", id).await?, Some(record.clone()));
    assert!(database.file_content(&first.id).await?.is_some());
    assert!(database.file_content(&second.id).await?.is_some());
    assert_eq!(
        database
            .file_info(&replacement.id)
            .await?
            .ok_or("回滚丢失暂存")?
            .record_id,
        None
    );

    let changed = database
        .update(
            "customers",
            id,
            json!({"attachments":[replacement]}),
            Some("a".repeat(64)),
        )
        .await?
        .ok_or("缺少替换结果")?;
    assert_eq!(changed["attachments"], json!([replacement]));
    assert!(database.file_info(&first.id).await?.is_none());
    assert!(database.file_content(&second.id).await?.is_none());
    assert_eq!(
        database
            .file_info(&replacement.id)
            .await?
            .ok_or("缺少替换附件")?
            .record_id,
        Some(id)
    );
    let cleared = database
        .update("customers", id, json!({"attachments":null}), None)
        .await?
        .ok_or("缺少清空结果")?;
    assert_eq!(cleared["attachments"], Value::Null);
    assert!(database.file_content(&replacement.id).await?.is_none());
    let last = database
        .stage_upload(attachment('a', "attachments", "删除.pdf"))
        .await?;
    database
        .update(
            "customers",
            id,
            json!({"attachments":[last]}),
            Some("a".repeat(64)),
        )
        .await?
        .ok_or("缺少更新结果")?;
    assert!(database.delete("customers", id).await?);
    assert!(database.file_info(&last.id).await?.is_none());
    assert!(database.file_content(&last.id).await?.is_none());
    database.shutdown().await?;
    Ok(())
}
