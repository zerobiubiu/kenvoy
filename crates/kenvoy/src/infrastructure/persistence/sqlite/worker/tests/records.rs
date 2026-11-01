use super::*;

#[tokio::test]
async fn sqlite_crud_is_persistent_validated_and_restricted_to_registered_resources() -> TestResult
{
    let fixture = Fixture::new()?;
    let app = fixture.application(BASIC)?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let info = database.info().await?;
    assert_eq!(info.journal_mode.to_lowercase(), "wal");
    assert!(info.foreign_keys);
    assert_eq!(info.busy_timeout_ms, 5000);
    assert!(!info.sqlite_version.is_empty());
    let first = database
        .create(
            "customers",
            json!({"name":"张三","phone":"13800000000","age":25}),
            None,
        )
        .await?;
    let id = first["id"].as_i64().ok_or("缺少 id")?;
    assert_eq!(first["name"], "张三");
    assert_eq!(first["age"], 25);
    assert_eq!(database.get("customers", id).await?, Some(first));
    let edited = database
        .update("customers", id, json!({"phone":null,"age":26}), None)
        .await?
        .ok_or("缺少更新结果")?;
    assert_eq!(edited["name"], "张三");
    assert_eq!(edited["phone"], Value::Null);
    assert_eq!(edited["age"], 26);
    for invalid in [
        json!({"name":" "}),
        json!({"name":"李四","age":-1}),
        json!({"name":"李四","age":1.5}),
        json!({"name":"李四","age":"25"}),
        json!({"name":"李四","id":7}),
    ] {
        assert!(matches!(
            database.create("customers", invalid, None).await,
            Err(DatabaseError::Validation(_))
        ));
    }
    assert!(matches!(
        database
            .update("customers", id, json!({"name":null}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    for name in [
        "sqlite_master",
        "_kenvoy_migrations",
        "customers; DROP TABLE customers",
        "missing",
    ] {
        assert!(matches!(
            database.list(name, ListQuery::default()).await,
            Err(DatabaseError::UnknownResource(_))
        ));
    }
    database.shutdown().await?;
    assert!(matches!(
        database.get("customers", id).await,
        Err(DatabaseError::Closed)
    ));
    let reopened = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(reopened.get("customers", id).await?, Some(edited));
    assert!(reopened.delete("customers", id).await?);
    assert!(!reopened.delete("customers", id).await?);
    assert!(reopened.get("customers", id).await?.is_none());
    assert!(
        reopened
            .update("customers", id, json!({"age":30}), None)
            .await?
            .is_none()
    );
    reopened.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn all_field_types_round_trip_and_defaults_do_not_overwrite_patch_absence() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application("<text name='name' required/><textarea name='note' default='默认'/><number name='score'/><boolean name='active' default={false}/><date name='day'/><datetime name='time'/><select name='state'><option value='new'/><option value='done'/></select>")?;
    let database = Database::open(fixture.database.clone(), app).await?;
    let record = database.create("customers", json!({"name":"含😀字符","score":1.25,"active":true,"day":"2024-02-29","time":"2024-02-29T08:00:00.123+08:00","state":"new"}), None).await?;
    let id = record["id"].as_i64().ok_or("缺少 id")?;
    assert_eq!(record["active"], true);
    assert_eq!(record["score"], 1.25);
    assert_eq!(record["note"], "默认");
    assert_eq!(record["time"], "2024-02-29T00:00:00.123Z");
    assert_eq!(
        database.update("customers", id, json!({}), None).await?,
        Some(record.clone())
    );
    let changed = database
        .update("customers", id, json!({"note":"修改"}), None)
        .await?
        .ok_or("缺少记录")?;
    assert_eq!(changed["active"], true);
    assert_eq!(changed["note"], "修改");
    let defaulted = database
        .create("customers", json!({"name":"默认布尔"}), None)
        .await?;
    assert_eq!(defaulted["active"], false);
    assert_eq!(defaulted["state"], Value::Null);
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn queries_escape_like_literals_validate_types_and_use_stable_pagination() -> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    for (name, age) in [
        ("100%", Some(1)),
        ("under_score", Some(2)),
        (r"back\slash", Some(3)),
        ("普通甲", None),
        ("普通乙", None),
        ("' OR 1=1 --", Some(4)),
    ] {
        database
            .create("customers", json!({"name":name,"age":age}), None)
            .await?;
    }
    for text in ["%", "_", "\\", "' OR 1=1 --"] {
        let result = database
            .list(
                "customers",
                ListQuery {
                    search: Some(text.into()),
                    ..Default::default()
                },
            )
            .await?;
        assert_eq!(result.total, 1, "LIKE 字符必须按字面量搜索：{text}");
    }
    let first = database
        .list(
            "customers",
            ListQuery {
                page_size: 2,
                sort: Some("age".into()),
                ..Default::default()
            },
        )
        .await?;
    let second = database
        .list(
            "customers",
            ListQuery {
                page: 2,
                page_size: 2,
                sort: Some("age".into()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(first.total, 6);
    assert_eq!(first.items.len(), 2);
    assert_eq!(second.items.len(), 2);
    assert!(
        first
            .items
            .iter()
            .all(|row| second.items.iter().all(|other| row["id"] != other["id"]))
    );
    assert!(
        first.items[0]["id"].as_i64() < first.items[1]["id"].as_i64(),
        "相同排序值按 id 稳定排序"
    );
    let nulls = database
        .list(
            "customers",
            ListQuery {
                filters: vec![FieldFilter {
                    field: "age".into(),
                    operator: FilterOperator::Eq,
                    value: Value::Null,
                }],
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(nulls.total, 2);
    let large = database
        .list(
            "customers",
            ListQuery {
                page: u32::MAX,
                page_size: 100,
                ..Default::default()
            },
        )
        .await?;
    assert!(large.items.is_empty());
    assert_eq!(large.total, 6);
    for query in [
        ListQuery {
            search: Some("x\0y".into()),
            ..Default::default()
        },
        ListQuery {
            filters: vec![FieldFilter {
                field: "name".into(),
                operator: FilterOperator::Contains,
                value: json!("x\0y"),
            }],
            ..Default::default()
        },
        ListQuery {
            page_size: 101,
            ..Default::default()
        },
        ListQuery {
            page: 0,
            ..Default::default()
        },
        ListQuery {
            sort: Some("name; DROP TABLE customers".into()),
            ..Default::default()
        },
        ListQuery {
            filters: vec![FieldFilter {
                field: "missing".into(),
                operator: FilterOperator::Eq,
                value: json!(1),
            }],
            ..Default::default()
        },
    ] {
        assert!(matches!(
            database.list("customers", query).await,
            Err(DatabaseError::InvalidQuery(_))
        ));
    }
    let typed = ListQuery {
        filters: vec![FieldFilter {
            field: "age".into(),
            operator: FilterOperator::Eq,
            value: json!("3"),
        }],
        ..Default::default()
    };
    assert!(matches!(
        database.list("customers", typed).await,
        Err(DatabaseError::Validation(_))
    ));
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn forged_resource_identifiers_are_rejected_before_database_creation() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = fixture.application(BASIC)?;
    app.resources[0].name = "customers; DROP TABLE secrets".into();
    assert!(matches!(
        Database::open(fixture.database.clone(), app).await,
        Err(DatabaseError::Migration(_))
    ));
    assert!(!fixture.database.exists());
    Ok(())
}

#[tokio::test]
async fn unsafe_generated_ids_roll_back_the_insert_and_sequence() -> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    database
        .create("customers", json!({"name":"已有记录"}), None)
        .await?;
    let connection = Connection::open(&fixture.database)?;
    connection.execute(
        "UPDATE sqlite_sequence SET seq=?1 WHERE name='customers'",
        [9_007_199_254_740_991_i64],
    )?;
    assert!(matches!(
        database
            .create("customers", json!({"name":"不应提交"}), None)
            .await,
        Err(DatabaseError::CorruptData(_))
    ));
    let count: i64 =
        connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))?;
    let sequence: i64 = connection.query_row(
        "SELECT seq FROM sqlite_sequence WHERE name='customers'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(count, 1);
    assert_eq!(sequence, 9_007_199_254_740_991);
    drop(connection);
    database.shutdown().await?;
    Ok(())
}
