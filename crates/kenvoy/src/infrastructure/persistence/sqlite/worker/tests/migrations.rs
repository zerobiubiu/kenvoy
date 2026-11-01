use super::*;

#[tokio::test]
async fn unique_conflicts_roll_back_and_failed_migrations_keep_previous_registry() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture
        .application("<text name='name' required unique/><number name='age' integer min={0}/>")?;
    let database = Database::open(fixture.database.clone(), app).await?;
    let one = database
        .create("customers", json!({"name":"甲","age":10}), None)
        .await?;
    let two = database
        .create("customers", json!({"name":"乙","age":20}), None)
        .await?;
    assert!(matches!(
        database
            .create("customers", json!({"name":"甲","age":30}), None)
            .await,
        Err(DatabaseError::Conflict)
    ));
    assert!(matches!(
        database
            .update(
                "customers",
                two["id"].as_i64().ok_or("缺少 id")?,
                json!({"name":"甲"}),
                None,
            )
            .await,
        Err(DatabaseError::Conflict)
    ));
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        2
    );
    let added = fixture.application("<text name='name' required unique/><number name='age' integer min={0}/><text name='phone' index/>")?;
    let outcome = database.replace_application(added, None).await?;
    assert!(outcome.applied);
    assert_eq!(
        database
            .get("customers", one["id"].as_i64().ok_or("缺少 id")?)
            .await?
            .ok_or("记录消失")?["phone"],
        Value::Null
    );
    let destructive = fixture.application("<text name='name' required unique/>")?;
    assert!(matches!(
        database.replace_application(destructive, None).await,
        Err(DatabaseError::Migration(_))
    ));
    let after = database
        .create(
            "customers",
            json!({"name":"丙","age":22,"phone":"123"}),
            None,
        )
        .await?;
    assert_eq!(after["age"], 22);
    assert_eq!(after["phone"], "123");
    assert_eq!(database.info().await?.generation, 2);
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn confirmed_schema_removal_prunes_blobs_and_failed_migrations_keep_attachments() -> TestResult
{
    let fixture = Fixture::new()?;
    let app = fixture.application(ATTACHMENTS)?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let removed = database
        .stage_upload(attachment('a', "attachments", "待删.pdf"))
        .await?;
    let kept = database
        .stage_upload(attachment('a', "secondary", "保留.pdf"))
        .await?;
    let pending = database
        .stage_upload(attachment('a', "attachments", "待删暂存.pdf"))
        .await?;
    let record = database
        .create(
            "customers",
            json!({"name":"迁移记录","attachments":[removed],"secondary":[kept]}),
            Some("a".repeat(64)),
        )
        .await?;
    let id = record["id"].as_i64().ok_or("缺少记录 id")?;
    let mut target = app.clone();
    target.resources[0]
        .fields
        .retain(|field| field.name != "attachments");
    let mut invalid = target.clone();
    invalid.resources[0]
        .fields
        .iter_mut()
        .find(|field| field.name == "secondary")
        .ok_or("缺少保留字段")?
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .max_size = 1;
    let failed_plan = migration::preview(&fixture.database, &invalid)?;
    assert!(matches!(
        database
            .replace_application(invalid, Some(failed_plan.checksum))
            .await,
        Err(DatabaseError::Migration(
            migration::MigrationError::InvalidData(_)
        ))
    ));
    assert_eq!(database.get("customers", id).await?, Some(record.clone()));
    assert!(database.file_content(&removed.id).await?.is_some());
    assert!(database.file_content(&pending.id).await?.is_some());
    assert!(matches!(
        database.replace_application(target.clone(), None).await,
        Err(DatabaseError::Migration(
            migration::MigrationError::ConfirmationRequired { .. }
        ))
    ));
    let plan = migration::preview(&fixture.database, &target)?;
    let outcome = database
        .replace_application(target.clone(), Some(plan.checksum))
        .await?;
    assert!(outcome.applied);
    assert!(database.file_info(&removed.id).await?.is_none());
    assert!(database.file_content(&pending.id).await?.is_none());
    assert!(database.file_content(&kept.id).await?.is_some());
    assert!(
        !database
            .get("customers", id)
            .await?
            .ok_or("迁移记录消失")?
            .contains_key("attachments")
    );
    let backup = Connection::open(outcome.backup.ok_or("破坏性迁移缺少备份")?)?;
    let backed_up: i64 =
        backup.query_row("SELECT count(*) FROM _kenvoy_uploads", [], |row| row.get(0))?;
    assert_eq!(backed_up, 3);
    drop(backup);
    target.resources.clear();
    let plan = migration::preview(&fixture.database, &target)?;
    database
        .replace_application(target, Some(plan.checksum))
        .await?;
    assert!(database.file_content(&kept.id).await?.is_none());
    database.shutdown().await?;
    Ok(())
}
