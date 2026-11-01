//! 阶段 00 的 HTTP/SQLite 集成探针，不是 Kenvoy 业务运行时。
//!
//! 路由、Schema 和身份开关仅用于验证所选依赖能协作；不代表通用 CRUD、
//! 迁移分析、生产认证或 React Runtime 已经实现。

#[path = "support/p03_sqlite.rs"]
mod database;
#[path = "support/p03_http_client.rs"]
mod http_client;
#[path = "support/p03_server.rs"]
mod server;

use database::{DbWorker, ScratchDirectory, apply_fixed_migration, count_customers, open_database};
use http_client::request;
use rusqlite::{Connection, ErrorCode, MAIN_DB, backup::NO_PROGRESS, params};
use serde_json::{Value, json};
use server::{ProbeServer, ProbeState};
use std::{
    error::Error,
    fs, io,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;

type ProbeResult<T = (), E = Box<dyn Error + Send + Sync>> = Result<T, E>;

#[test]
fn sqlite_file_transactions_migration_replay_wal_and_contention() -> ProbeResult {
    let scratch = ScratchDirectory::new()?;
    let scratch_path = scratch.path().to_path_buf();
    let database_path = scratch.path().join("app.sqlite");
    let mut connection = open_database(&database_path, Duration::from_millis(150))?;
    let sqlite_version = rusqlite::version_number();
    assert!(
        sqlite_version >= 3_051_003
            || (3_050_007..3_051_000).contains(&sqlite_version)
            || (3_044_006..3_045_000).contains(&sqlite_version),
        "所选 SQLite 内核需要包含官方 WAL-reset 修复"
    );
    assert_eq!(
        connection.pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        connection.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?,
        "wal"
    );
    assert_eq!(
        connection.pragma_query_value(None, "busy_timeout", |row| row.get::<_, i64>(0))?,
        150
    );

    apply_fixed_migration(&mut connection)?;
    connection.execute(
        "INSERT INTO customers(name, age) VALUES (?1, ?2)",
        params!["张三", 25],
    )?;
    apply_fixed_migration(&mut connection)?;
    assert_eq!(count_customers(&connection)?, 1);
    assert_eq!(
        connection.query_row("SELECT count(*) FROM _probe_migrations", [], |row| {
            row.get::<_, i64>(0)
        })?,
        1
    );

    // 数据操作失败后主动回滚；已插入的事务内记录不能残留。
    let transaction = connection.transaction()?;
    transaction.execute("INSERT INTO customers(name, age) VALUES ('应回滚', 20)", [])?;
    assert!(
        transaction
            .execute(
                "INSERT INTO customers(name, age) VALUES ('非法年龄', -1)",
                []
            )
            .is_err()
    );
    transaction.rollback()?;
    assert_eq!(count_customers(&connection)?, 1);

    // DDL 与迁移记录同事务，失败时连新增列也不能留下。
    let transaction = connection.transaction()?;
    transaction.execute_batch("ALTER TABLE customers ADD COLUMN phone TEXT;")?;
    assert!(
        transaction
            .execute(
                "INSERT INTO _probe_migrations(version, checksum) VALUES (1, '重复版本')",
                [],
            )
            .is_err()
    );
    drop(transaction);
    assert_eq!(
        connection.query_row(
            "SELECT count(*) FROM pragma_table_info('customers') WHERE name = 'phone'",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    assert!(
        connection
            .execute(
                "INSERT INTO customer_notes(customer_id, note) VALUES (999, '缺失父记录')",
                [],
            )
            .is_err()
    );

    let other = open_database(&database_path, Duration::from_millis(150))?;
    connection.execute_batch("BEGIN IMMEDIATE;")?;
    connection.execute("INSERT INTO customers(name, age) VALUES ('未提交', 30)", [])?;
    // WAL 下第二连接仍可读到已提交快照，但并行写者受忙锁限制。
    assert_eq!(count_customers(&other)?, 1);
    let began_wait = Instant::now();
    let busy_error = other
        .execute("INSERT INTO customers(name, age) VALUES ('竞争写', 30)", [])
        .expect_err("并行写者应受到 SQLite 锁约束");
    assert!(matches!(
        busy_error.sqlite_error_code(),
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    ));
    println!(
        "P03 SQLite={} WAL=wal FK=1 busy_timeout=150ms；竞争写返回 {:?}，等待 {:?}",
        rusqlite::version(),
        busy_error.sqlite_error_code(),
        began_wait.elapsed()
    );
    connection.execute_batch("ROLLBACK;")?;
    drop(other);
    drop(connection);

    let writers: Vec<_> = (0..4)
        .map(|writer| {
            let database_path = database_path.clone();
            thread::spawn(move || -> rusqlite::Result<()> {
                let connection = open_database(&database_path, Duration::from_secs(5))?;
                for row in 0..10 {
                    connection.execute(
                        "INSERT INTO customers(name, age) VALUES (?1, ?2)",
                        params![format!("并发{writer}-{row}"), row],
                    )?;
                }
                Ok(())
            })
        })
        .collect();
    for writer in writers {
        writer
            .join()
            .map_err(|_| io::Error::other("SQLite 并发探针线程异常退出"))??;
    }
    let reopened = open_database(&database_path, Duration::from_secs(5))?;
    assert_eq!(count_customers(&reopened)?, 41);
    assert_eq!(
        reopened.query_row("SELECT name FROM customers WHERE id = 1", [], |row| {
            row.get::<_, String>(0)
        })?,
        "张三"
    );
    let backup_path = scratch.path().join("backup.sqlite");
    reopened.backup(MAIN_DB, &backup_path, NO_PROGRESS)?;
    let backup = Connection::open(&backup_path)?;
    assert_eq!(count_customers(&backup)?, 41);
    drop(backup);
    drop(reopened);
    println!("P03 数据回滚/DDL回滚/迁移幂等/FK/四写者40条/重开持久化/在线备份均通过");
    drop(scratch);
    assert!(!scratch_path.exists(), "SQLite 临时文件必须释放并清理");
    Ok(())
}

#[tokio::test]
async fn shutdown_timeout_aborts_and_reaps_the_owned_server_task() -> ProbeResult {
    use std::sync::atomic::AtomicBool;
    struct Released(Arc<AtomicBool>);
    impl Drop for Released {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let scratch = ScratchDirectory::new()?;
    let released = Arc::new(AtomicBool::new(false));
    let owned = Released(Arc::clone(&released));
    let task = tokio::spawn(async move {
        let _owned = owned;
        std::future::pending::<io::Result<()>>().await
    });
    let database = Arc::new(DbWorker::start(scratch.path().join("app.sqlite"))?);
    let server = ProbeServer {
        address: "127.0.0.1:0".parse()?,
        shutdown: None,
        task: Some(task),
        database,
    };
    let error = server
        .finish_with_deadline(Duration::from_millis(10))
        .await
        .expect_err("不合作任务必须触发关闭期限");
    assert!(error.to_string().contains("已中止并回收"));
    assert!(
        released.load(Ordering::SeqCst),
        "被中止任务持有的资源必须释放"
    );
    println!("P03 注入不合作服务任务：deadline触发abort并确认任务资源已释放");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_loopback_serves_sqlite_api_and_assets_with_default_deny() -> ProbeResult {
    let scratch = ScratchDirectory::new()?;
    let scratch_path = scratch.path().to_path_buf();
    fs::create_dir_all(scratch.path().join("client/assets"))?;
    fs::write(
        scratch.path().join("client/assets/probe.js"),
        "globalThis.p03Asset = true;",
    )?;
    fs::write(
        scratch.path().join("client/private.txt"),
        "P03_PRIVATE_SENTINEL",
    )?;
    let database_path = scratch.path().join("app.sqlite");
    let database = Arc::new(DbWorker::start(database_path.clone())?);
    let admin_token: Arc<str> = format!("Bearer p03-{}", scratch_path.display()).into();
    let server = ProbeServer::start(
        scratch.path(),
        ProbeState {
            database: Arc::clone(&database),
            admin_token: Arc::clone(&admin_token),
        },
    )
    .await?;
    let address = server.address;

    let page = request(address, "GET", "/customers", "", None).await?;
    assert_eq!(page.status, 200);
    assert!(page.body.contains("p03-probe-shell"));
    let asset = request(address, "GET", "/assets/probe.js", "", None).await?;
    assert_eq!(asset.status, 200);
    assert!(asset.body.contains("p03Asset"));
    for path in [
        "/api/not_registered",
        "/missing-page",
        "/assets/missing.js",
        "/assets/../private.txt",
        "/assets/%2e%2e/private.txt",
        "/assets/%2e%2e%2fprivate.txt",
        "/app.sqlite",
    ] {
        let response = request(address, "GET", path, "", None).await?;
        assert!(matches!(response.status, 400 | 404), "{path}");
        assert!(!response.body.contains("p03-probe-shell"), "{path}");
        assert!(!response.body.contains("P03_PRIVATE_SENTINEL"), "{path}");
    }
    let invalid_api = request(address, "GET", "/api/not_registered", "", None).await?;
    assert!(invalid_api.headers.contains("application/json"));
    assert_eq!(
        request(address, "DELETE", "/api/customers", "", None)
            .await?
            .status,
        405
    );
    for path in ["/api/customers", "/admin/customers"] {
        assert_eq!(request(address, "GET", path, "", None).await?.status, 401);
        assert_eq!(
            request(address, "GET", path, "", Some("Bearer incorrect"))
                .await?
                .status,
            401
        );
    }
    for body in [
        r#"{"name":"","age":25}"#,
        r#"{"name":"张三","age":-1}"#,
        r#"{"name":"张三","age":2.5}"#,
        r#"{"name":"张三","age":9007199254740992}"#,
        r#"{"name":"张三","age":25,"role":"admin"}"#,
        r#"{"name":"张三","name":"重复键"}"#,
    ] {
        assert_eq!(
            request(address, "POST", "/api/customers", body, None)
                .await?
                .status,
            422
        );
    }
    let too_long = json!({ "name": "a".repeat(101) }).to_string();
    assert_eq!(
        request(address, "POST", "/api/customers", &too_long, None)
            .await?
            .status,
        422
    );
    assert_eq!(
        request(address, "POST", "/api/customers", "{", None)
            .await?
            .status,
        400
    );
    let too_big = json!({ "name": "x".repeat(1024) }).to_string();
    assert_eq!(
        request(address, "POST", "/api/customers", &too_big, None)
            .await?
            .status,
        413
    );

    let created = request(
        address,
        "POST",
        "/api/customers",
        r#"{"name":"张三","age":25}"#,
        None,
    )
    .await?;
    assert_eq!(created.status, 201);
    let created: Value = serde_json::from_str(&created.body)?;
    assert_eq!(created, json!({ "id": 1, "name": "张三", "age": 25 }));
    let injection = json!({ "name": "张三'); DROP TABLE customers;--", "age": 30 });
    let injection_result = request(
        address,
        "POST",
        "/api/customers",
        &injection.to_string(),
        None,
    )
    .await?;
    assert_eq!(injection_result.status, 201);
    assert_eq!(
        serde_json::from_str::<Value>(&injection_result.body)?["name"],
        injection["name"]
    );
    let mut pending = Vec::new();
    for index in 0..6 {
        pending.push(tokio::spawn(async move {
            let body = json!({ "name": format!("HTTP并发{index}"), "age": index }).to_string();
            request(address, "POST", "/api/customers", &body, None).await
        }));
    }
    for response in pending {
        assert_eq!(response.await??.status, 201);
    }
    let count = request(address, "GET", "/admin/customers", "", Some(&admin_token)).await?;
    assert_eq!(count.status, 200);
    assert_eq!(serde_json::from_str::<Value>(&count.body)?["count"], 8);
    server.finish().await?;
    assert!(TcpStream::connect(address).await.is_err());
    drop(database);
    let reopened = open_database(&database_path, Duration::from_secs(5))?;
    assert_eq!(count_customers(&reopened)?, 8);
    drop(reopened);
    println!(
        "P03 Axum loopback：同端口页面/静态资源/真实SQLite写入；匿名与错误身份拒绝；404/405/413/422；路径遍历/SQL注入；6并发；优雅退出与重开8条通过"
    );
    drop(scratch);
    assert!(!scratch_path.exists(), "HTTP/SQLite 临时资源必须清理");
    Ok(())
}
