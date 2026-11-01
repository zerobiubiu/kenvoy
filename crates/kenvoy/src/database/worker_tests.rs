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
        )
        .await?;
    let id = first["id"].as_i64().ok_or("缺少 id")?;
    assert_eq!(first["name"], "张三");
    assert_eq!(first["age"], 25);
    assert_eq!(database.get("customers", id).await?, Some(first));
    let edited = database
        .update("customers", id, json!({"phone":null,"age":26}))
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
            database.create("customers", invalid).await,
            Err(DatabaseError::Validation(_))
        ));
    }
    assert!(matches!(
        database.update("customers", id, json!({"name":null})).await,
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
            .update("customers", id, json!({"age":30}))
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
    let record = database.create("customers", json!({"name":"含😀字符","score":1.25,"active":true,"day":"2024-02-29","time":"2024-02-29T08:00:00.123+08:00","state":"new"})).await?;
    let id = record["id"].as_i64().ok_or("缺少 id")?;
    assert_eq!(record["active"], true);
    assert_eq!(record["score"], 1.25);
    assert_eq!(record["note"], "默认");
    assert_eq!(record["time"], "2024-02-29T00:00:00.123Z");
    assert_eq!(
        database.update("customers", id, json!({})).await?,
        Some(record.clone())
    );
    let changed = database
        .update("customers", id, json!({"note":"修改"}))
        .await?
        .ok_or("缺少记录")?;
    assert_eq!(changed["active"], true);
    assert_eq!(changed["note"], "修改");
    let defaulted = database
        .create("customers", json!({"name":"默认布尔"}))
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
            .create("customers", json!({"name":name,"age":age}))
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
        .create("customers", json!({"name":"甲","age":10}))
        .await?;
    let two = database
        .create("customers", json!({"name":"乙","age":20}))
        .await?;
    assert!(matches!(
        database
            .create("customers", json!({"name":"甲","age":30}))
            .await,
        Err(DatabaseError::Conflict)
    ));
    assert!(matches!(
        database
            .update(
                "customers",
                two["id"].as_i64().ok_or("缺少 id")?,
                json!({"name":"甲"})
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
        .create("customers", json!({"name":"丙","age":22,"phone":"123"}))
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
            .create("customers", json!({"name":"队列拒绝"}))
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
            .create("customers", json!({"name":"响应丢弃仍需核对"}))
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
        .create("customers", json!({"name":"已有记录"}))
        .await?;
    let connection = Connection::open(&fixture.database)?;
    connection.execute(
        "UPDATE sqlite_sequence SET seq=?1 WHERE name='customers'",
        [9_007_199_254_740_991_i64],
    )?;
    assert!(matches!(
        database
            .create("customers", json!({"name":"不应提交"}))
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
        .create("customers", json!({"name":"关闭后保留"}))
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
                    .create("customers", json!({"name":"并行生命周期"}))
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
                    .create("customers", json!({"name":"重新写入"}))
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
