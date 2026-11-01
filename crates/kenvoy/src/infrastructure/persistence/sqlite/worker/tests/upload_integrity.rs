use super::*;

#[tokio::test]
async fn upload_references_cannot_cross_sessions_fields_resources_or_records() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = fixture.application(ATTACHMENTS)?;
    let mut other_resource = app.resources[0].clone();
    other_resource.name = "accounts".into();
    app.resources.push(other_resource);
    let database = Database::open(fixture.database.clone(), app).await?;
    let file = database
        .stage_upload(attachment('a', "attachments", "会话.pdf"))
        .await?;
    for (resource, value, owner) in [
        (
            "customers",
            json!({"name":"他人会话","attachments":[file]}),
            Some("b".repeat(64)),
        ),
        (
            "customers",
            json!({"name":"无会话","attachments":[file]}),
            None,
        ),
        (
            "customers",
            json!({"name":"跨字段","secondary":[file]}),
            Some("a".repeat(64)),
        ),
        (
            "accounts",
            json!({"name":"跨资源","attachments":[file]}),
            Some("a".repeat(64)),
        ),
    ] {
        assert!(matches!(
            database.create(resource, value, owner).await,
            Err(DatabaseError::Validation(_))
        ));
    }
    let mut forged = serde_json::to_value(&file)?;
    forged["name"] = "伪造.pdf".into();
    assert!(matches!(
        database
            .create(
                "customers",
                json!({"name":"伪造元数据","attachments":[forged]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert!(matches!(
        database
            .create(
                "customers",
                json!({"name":"重复引用","attachments":[file,file]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert!(matches!(
        database
            .create(
                "customers",
                json!({"name":"超数量","attachments":[file,file,file]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        0
    );
    assert_eq!(
        database.list("accounts", ListQuery::default()).await?.total,
        0
    );
    assert_eq!(
        database
            .file_info(&file.id)
            .await?
            .ok_or("失败写入丢失附件")?
            .record_id,
        None
    );
    let first = database
        .create(
            "customers",
            json!({"name":"拥有者","attachments":[file]}),
            Some("a".repeat(64)),
        )
        .await?;
    let first_id = first["id"].as_i64().ok_or("缺少记录 id")?;
    let second = database
        .create("customers", json!({"name":"其他记录"}), None)
        .await?;
    let second_id = second["id"].as_i64().ok_or("缺少记录 id")?;
    assert!(matches!(
        database
            .create(
                "customers",
                json!({"name":"重复绑定","attachments":[file]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert!(matches!(
        database
            .update(
                "customers",
                second_id,
                json!({"attachments":[file]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert!(matches!(
        database
            .update(
                "customers",
                first_id,
                json!({"secondary":[file]}),
                Some("a".repeat(64))
            )
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database.get("customers", first_id).await?,
        Some(first.clone())
    );
    assert_eq!(database.get("customers", second_id).await?, Some(second));
    let retained = database
        .update(
            "customers",
            first_id,
            json!({"name":"保留现有附件","attachments":[file]}),
            Some("b".repeat(64)),
        )
        .await?
        .ok_or("缺少保留结果")?;
    assert_eq!(retained["attachments"], first["attachments"]);
    assert_eq!(
        database
            .file_info(&file.id)
            .await?
            .ok_or("保留附件消失")?
            .record_id,
        Some(first_id)
    );
    database.shutdown().await?;
    Ok(())
}
