use super::*;
use crate::database::{FieldFilter, FilterOperator};
use serde_json::json;
use std::{
    error::Error,
    sync::atomic::AtomicUsize,
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    database: PathBuf,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "kenvoy-database-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root)?;
        fs::create_dir(root.join("src"))?;
        Ok(Self {
            database: root.join("data/app.sqlite"),
            root,
        })
    }
    fn application(&self, fields: &str) -> TestResult<ApplicationIR> {
        let entry = self.root.join("src/app.tsx");
        fs::write(
            &entry,
            format!(
                "export default <app name='数据库验证'><resource name='customers'>{fields}</resource></app>;"
            ),
        )?;
        let program = kengine::Engine::new().analyze_project(
            &self.root,
            kengine::AnalyzeOptions {
                entry_points: vec![entry.clone()],
                ..Default::default()
            },
        )?;
        if program.has_errors() {
            return Err(format!("解析失败：{:?}", program.diagnostics).into());
        }
        let analyzed = crate::dsl::analyze(&program, &entry.canonicalize()?, "数据库验证");
        analyzed
            .application
            .ok_or_else(|| format!("DSL 失败：{:?}", analyzed.diagnostics).into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

const BASIC: &str = "<text name='name' required maxLength={100}/><text name='phone'/><number name='age' integer min={0}/>";

#[tokio::test]
async fn linked_options_compare_canonical_numbers_and_datetimes() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(r#"<number name='count' integer/><number name='measure'/><datetime name='occurred'/><select name='choice'><option value='yes' when={{count:1.0,measure:1,occurred:'2026-11-01T08:00:00+08:00'}}/></select>"#)?;
    let database = Database::open(fixture.database.clone(), app).await?;
    let created = database
        .create(
            "customers",
            json!({"count":1,"measure":1.0,"occurred":"2026-11-01T00:00:00Z","choice":"yes"}),
            None,
        )
        .await?;
    assert_eq!(created["occurred"], "2026-11-01T00:00:00.000Z");
    let id = created["id"].as_i64().ok_or("缺少 id")?;
    assert!(matches!(
        database
            .update("customers", id, json!({"count":2}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database.get("customers", id).await?.ok_or("缺少原记录")?,
        created
    );
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn linked_and_resource_options_validate_inside_write_transactions() -> TestResult {
    let fixture = Fixture::new()?;
    let entry = fixture.root.join("src/app.tsx");
    fs::write(
        &entry,
        r#"export default <app name='联动'><resource name='cities'><text name='code' unique required/><text name='name' required/><text name='province' required/><text name='secret'/></resource><resource name='customers'><select name='province' required><option value='zj'/><option value='js'/></select><select name='city' required optionsSource={{resource:'cities',value:'code',label:'name',filters:{province:'province'},permission:'anonymous'}}/><select name='district'><option value='xihu' when={{province:'zj'}}/><option value='gulou' when={{province:'js'}}/></select></resource></app>;"#,
    )?;
    let program = kengine::Engine::new().analyze_project(
        &fixture.root,
        kengine::AnalyzeOptions {
            entry_points: vec![entry.clone()],
            ..Default::default()
        },
    )?;
    let result = crate::dsl::analyze(&program, &entry.canonicalize()?, "选项");
    let app = result
        .application
        .ok_or_else(|| format!("{:?}", result.diagnostics))?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let mut labels_only = app.clone();
    let linked = labels_only
        .resources
        .iter_mut()
        .find(|resource| resource.name == "customers")
        .ok_or("缺少客户资源")?;
    linked
        .fields
        .iter_mut()
        .find(|field| field.name == "district")
        .ok_or("缺少区字段")?
        .options[0]
        .label = "西湖区".into();
    linked
        .fields
        .iter_mut()
        .find(|field| field.name == "district")
        .ok_or("缺少区字段")?
        .options[0]
        .when
        .insert("province".into(), json!("js"));
    assert_eq!(
        migration::schema_model(&labels_only)?,
        migration::schema_model(&app)?
    );
    assert!(!migration::preview(&fixture.database, &labels_only)?.destructive);
    let mut fixed = app.clone();
    let remote = fixed
        .resources
        .iter_mut()
        .find(|resource| resource.name == "customers")
        .and_then(|resource| {
            resource
                .fields
                .iter_mut()
                .find(|field| field.name == "city")
        })
        .ok_or("缺少城市字段")?;
    remote.options_source = None;
    remote.options.push(crate::application::SelectOption {
        value: "hz".into(),
        label: "杭州".into(),
        when: Default::default(),
    });
    assert!(migration::preview(&fixture.database, &fixed)?.destructive);
    let hz = database
        .create(
            "cities",
            json!({"code":"hz","name":"杭州","province":"zj","secret":"私有"}),
            None,
        )
        .await?;
    database
        .create(
            "cities",
            json!({"code":"nb","name":"宁波","province":"zj"}),
            None,
        )
        .await?;
    database
        .create(
            "cities",
            json!({"code":"nj","name":"南京","province":"js"}),
            None,
        )
        .await?;
    let request: super::super::OptionsQuery = serde_json::from_value(
        json!({"dependencies":{"province":"zj"},"page_size":1,"selected":"hz"}),
    )?;
    let result = database.options("customers", "city", request).await?;
    assert_eq!(result.total, 2);
    assert_eq!(result.data.len(), 1);
    assert_eq!(result.selected.ok_or("缺少选中项")?.label, "杭州");
    let result = database
        .options(
            "customers",
            "city",
            serde_json::from_value(json!({"dependencies":{"province":"js"},"selected":"hz"}))?,
        )
        .await?;
    assert!(result.selected.is_none());
    assert_eq!(result.data[0].value, "nj");
    for invalid in [
        json!({"dependencies":{"secret":"私有"}}),
        json!({"dependencies":{},"page_size":101}),
        json!({"dependencies":{},"page":0}),
    ] {
        assert!(
            database
                .options("customers", "city", serde_json::from_value(invalid)?)
                .await
                .is_err()
        );
    }
    let empty = database
        .options("customers", "city", Default::default())
        .await?;
    assert_eq!(empty.total, 0);
    let created = database
        .create(
            "customers",
            json!({"province":"zj","city":"hz","district":"xihu"}),
            None,
        )
        .await?;
    let id = created["id"].as_i64().ok_or("缺少 id")?;
    for invalid in [
        json!({"province":"js","city":"hz"}),
        json!({"province":"zj","city":"missing"}),
        json!({"province":"zj","city":"hz","district":"gulou"}),
    ] {
        assert!(matches!(
            database.create("customers", invalid, None).await,
            Err(DatabaseError::Validation(_))
        ));
    }
    assert!(matches!(
        database
            .update("customers", id, json!({"province":"js"}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database.get("customers", id).await?.ok_or("记录丢失")?,
        created
    );
    database
        .update(
            "customers",
            id,
            json!({"province":"js","city":"nj","district":"gulou"}),
            None,
        )
        .await?;
    database
        .delete("cities", hz["id"].as_i64().ok_or("缺少源 id")?)
        .await?;
    assert!(matches!(
        database
            .create("customers", json!({"province":"zj","city":"hz"}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    database.shutdown().await?;
    let reopened = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(
        reopened.get("customers", id).await?.ok_or("持久化失败")?["city"],
        "nj"
    );
    reopened.shutdown().await?;
    Ok(())
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_requests_use_one_connection_and_queue_rejection_is_known_not_executed()
-> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let owner = database.clone();
    let blocking = tokio::spawn(async move {
        owner
            .submit(move |_| {
                let _ = started.send(());
                blocked
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| DatabaseError::InvalidState("测试未释放阻塞任务".into()))?;
                Ok(())
            })
            .await
    });
    ready.await?;
    let count = Arc::new(AtomicUsize::new(0));
    let mut responses = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
        let count = count.clone();
        let (reply, response) = oneshot::channel();
        database
            .shared
            .sender
            .try_send(Command::Run(Box::new(move |_, cancelled| {
                if !cancelled {
                    count.fetch_add(1, Ordering::SeqCst);
                }
                let _ = reply.send(cancelled);
            })))
            .map_err(|_| "队列容量不正确")?;
        responses.push(response);
    }
    assert!(matches!(
        database
            .create("customers", json!({"name":"队列拒绝"}), None)
            .await,
        Err(DatabaseError::Busy)
    ));
    release.send(())?;
    blocking.await??;
    for response in responses {
        assert!(!response.await?);
    }
    assert_eq!(count.load(Ordering::SeqCst), QUEUE_CAPACITY);
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        0
    );
    let mut writes = Vec::new();
    for index in 0..20 {
        let owner = database.clone();
        writes.push(tokio::spawn(async move {
            owner
                .create(
                    "customers",
                    json!({"name":format!("并发{index}"),"age":index}),
                    None,
                )
                .await
        }));
    }
    for write in writes {
        write.await??;
    }
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        20
    );
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
async fn dropping_an_accepted_write_response_does_not_claim_the_write_was_cancelled() -> TestResult
{
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let owner = database.clone();
    let blocking = tokio::spawn(async move {
        owner
            .submit(move |_| {
                let _ = started.send(());
                blocked
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| DatabaseError::InvalidState("测试未释放阻塞任务".into()))?;
                Ok(())
            })
            .await
    });
    ready.await?;
    let owner = database.clone();
    let accepted = tokio::spawn(async move {
        owner
            .create("customers", json!({"name":"响应丢弃仍需核对"}), None)
            .await
    });
    for _ in 0..100 {
        if database.shared.sender.capacity() < QUEUE_CAPACITY {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(database.shared.sender.capacity(), QUEUE_CAPACITY - 1);
    accepted.abort();
    let _ = accepted.await;
    release.send(())?;
    blocking.await??;
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        1
    );
    database.shutdown().await?;
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

#[tokio::test]
async fn shutdown_reaps_connection_is_idempotent_and_closes_every_clone() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(BASIC)?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let clone = database.clone();
    database
        .create("customers", json!({"name":"关闭后保留"}), None)
        .await?;
    tokio::time::timeout(Duration::from_secs(2), database.shutdown()).await??;
    assert!(
        database
            .shared
            .thread
            .lock()
            .map_err(|_| "线程句柄锁损坏")?
            .is_none()
    );
    clone.shutdown().await?;
    assert!(matches!(clone.info().await, Err(DatabaseError::Closed)));
    let reopened = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(
        reopened
            .list("customers", ListQuery::default())
            .await?
            .total,
        1
    );
    reopened.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn shutdown_interrupts_long_running_sql_and_reaps_the_owned_thread() -> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let owner = database.clone();
    let running = tokio::spawn(async move {
        owner.submit(move |state| {
        let _ = started.send(());
        state.connection.query_row("WITH RECURSIVE series(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM series) SELECT sum(n) FROM series", [], |row| row.get::<_, i64>(0)).map_err(DatabaseError::from)
    }).await
    });
    ready.await?;
    tokio::time::timeout(Duration::from_secs(14), database.shutdown()).await??;
    assert!(
        database
            .shared
            .thread
            .lock()
            .map_err(|_| "线程句柄锁损坏")?
            .is_none()
    );
    assert!(
        matches!(running.await?, Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))) if error.code == rusqlite::ErrorCode::OperationInterrupted)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_independent_databases_reopen_without_owning_old_connections() -> TestResult {
    let mut tasks = Vec::new();
    for _ in 0..8 {
        tasks.push(tokio::spawn(async move {
            for _ in 0..6 {
                let fixture = Fixture::new()?;
                let app = fixture.application(BASIC)?;
                let database = Database::open(fixture.database.clone(), app.clone())
                    .await
                    .map_err(|error| format!("首次打开 {}：{error}", fixture.database.display()))?;
                database
                    .create("customers", json!({"name":"并行生命周期"}), None)
                    .await
                    .map_err(|error| format!("首次写入 {}：{error}", fixture.database.display()))?;
                database
                    .shutdown()
                    .await
                    .map_err(|error| format!("关闭 {}：{error}", fixture.database.display()))?;
                let reopened = Database::open(fixture.database.clone(), app)
                    .await
                    .map_err(|error| format!("重新打开 {}：{error}", fixture.database.display()))?;
                assert_eq!(
                    reopened
                        .list("customers", ListQuery::default())
                        .await?
                        .total,
                    1
                );
                reopened
                    .create("customers", json!({"name":"重新写入"}), None)
                    .await
                    .map_err(|error| format!("重新写入 {}：{error}", fixture.database.display()))?;
                reopened.shutdown().await?;
            }
            Ok::<_, Box<dyn Error + Send + Sync>>(())
        }));
    }
    for task in tasks {
        task.await??;
    }
    Ok(())
}

fn attachment(owner: char, field: &str, name: &str) -> crate::database::NewUpload {
    crate::database::NewUpload {
        resource: "customers".into(),
        field: field.into(),
        owner: owner.to_string().repeat(64),
        name: name.into(),
        content_type: "application/pdf".into(),
        data: b"%PDF-1.7\nbinary".to_vec(),
        expires_at: crate::database::uploads::now().expect("测试时钟有效") + 3600,
    }
}

const ATTACHMENTS: &str = "<text name='name' required unique/><file name='attachments' maxFiles={2} accept={['application/pdf']}/><file name='secondary' maxFiles={2}/>";

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
