//! 阶段 00 的 HTTP/SQLite 集成探针，不是 Kenvoy 业务运行时。
//!
//! 路由、Schema 和身份开关仅用于验证所选依赖能协作；不代表通用 CRUD、
//! 迁移分析、生产认证或 React Runtime 已经实现。

use std::{
    error::Error,
    fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
};
use rusqlite::{Connection, ErrorCode, MAIN_DB, OptionalExtension, backup::NO_PROGRESS, params};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir};

type ProbeResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
type DbReply = oneshot::Sender<Result<Value, String>>;

struct AbortServerOnDrop(tokio::task::AbortHandle);

impl Drop for AbortServerOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

const SCHEMA_CHECKSUM: &str = "p03-fixed-schema-v1";
const SHELL: &str = "<!doctype html><div id=\"p03-probe-shell\"></div>";
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    fn new() -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let serial = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kenvoy-p03-{}-{stamp}-{serial}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        // 只清理本测试创建的、已核验位于临时目录内的独立目录。
        let target = fs::canonicalize(&self.0);
        let parent = fs::canonicalize(std::env::temp_dir());
        if let (Ok(target), Ok(parent)) = (target, parent) {
            let expected_name = target
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("kenvoy-p03-"));
            if target.parent() == Some(parent.as_path())
                && expected_name
                && let Err(error) = fs::remove_dir_all(&target)
            {
                eprintln!("P03 临时目录清理失败：{error}");
            }
        }
    }
}

fn open_database(path: &Path, timeout: Duration) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.busy_timeout(timeout)?;
    Ok(connection)
}

fn apply_fixed_migration(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS _probe_migrations (
             version INTEGER PRIMARY KEY,
             checksum TEXT NOT NULL
         );",
    )?;
    let previous: Option<String> = transaction
        .query_row(
            "SELECT checksum FROM _probe_migrations WHERE version = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(previous) = previous {
        if previous != SCHEMA_CHECKSUM {
            return Err(rusqlite::Error::InvalidQuery);
        }
    } else {
        transaction.execute_batch(
            "CREATE TABLE customers (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                 age INTEGER CHECK(age >= 0)
             );
             CREATE TABLE customer_notes (
                 id INTEGER PRIMARY KEY,
                 customer_id INTEGER NOT NULL REFERENCES customers(id),
                 note TEXT NOT NULL
             );
             CREATE INDEX customers_name_idx ON customers(name);",
        )?;
        transaction.execute(
            "INSERT INTO _probe_migrations(version, checksum) VALUES (1, ?1)",
            [SCHEMA_CHECKSUM],
        )?;
    }
    transaction.commit()
}

fn count_customers(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))
}

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

enum DbCommand {
    Create {
        name: String,
        age: Option<i64>,
        reply: DbReply,
    },
    Count(DbReply),
    Stop,
}

struct DbWorker {
    sender: SyncSender<DbCommand>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl DbWorker {
    fn start(path: PathBuf) -> ProbeResult<Self> {
        let (sender, receiver) = mpsc::sync_channel(8);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let prepared = open_database(&path, Duration::from_secs(5)).and_then(|mut database| {
                apply_fixed_migration(&mut database)?;
                Ok(database)
            });
            let database = match prepared {
                Ok(database) => {
                    let _ = ready_sender.send(Ok::<_, String>(()));
                    database
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(error.to_string()));
                    return;
                }
            };
            while let Ok(command) = receiver.recv() {
                match command {
                    DbCommand::Create { name, age, reply } => {
                        let result = (|| -> rusqlite::Result<Value> {
                            database.execute(
                                "INSERT INTO customers(name, age) VALUES (?1, ?2)",
                                params![name, age],
                            )?;
                            database.query_row(
                                "SELECT id, name, age FROM customers WHERE id = ?1",
                                [database.last_insert_rowid()],
                                |row| {
                                    Ok(json!({
                                        "id": row.get::<_, i64>(0)?,
                                        "name": row.get::<_, String>(1)?,
                                        "age": row.get::<_, Option<i64>>(2)?
                                    }))
                                },
                            )
                        })();
                        let _ = reply.send(result.map_err(|error| error.to_string()));
                    }
                    DbCommand::Count(reply) => {
                        let _ = reply.send(
                            count_customers(&database)
                                .map(|count| json!({ "count": count }))
                                .map_err(|error| error.to_string()),
                        );
                    }
                    DbCommand::Stop => break,
                }
            }
        });
        match ready_receiver.recv()? {
            Ok(()) => Ok(Self {
                sender,
                thread: Mutex::new(Some(thread)),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(io::Error::other(error).into())
            }
        }
    }

    fn stop(&self) {
        if let Ok(mut thread) = self.thread.lock()
            && let Some(thread) = thread.take()
        {
            let _ = self.sender.send(DbCommand::Stop);
            let _ = thread.join();
        }
    }
}

impl Drop for DbWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct ProbeState {
    database: Arc<DbWorker>,
    // 只验证服务端身份门控；此探针开关不构成正式 Session 实现。
    admin_token: Arc<str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCustomer {
    name: String,
    age: Option<i64>,
}

type ApiResult = Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)>;

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message })))
}

async fn create_customer(
    State(state): State<ProbeState>,
    Json(input): Json<CreateCustomer>,
) -> ApiResult {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 100
        || input
            .age
            .is_some_and(|age| !(0..=MAX_SAFE_INTEGER).contains(&age))
    {
        return Err(api_error(StatusCode::UNPROCESSABLE_ENTITY, "字段校验失败"));
    }
    let (reply, receiver) = oneshot::channel();
    state
        .database
        .sender
        .try_send(DbCommand::Create {
            name: input.name,
            age: input.age,
            reply,
        })
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库队列不可用"))?;
    let record = receiver
        .await
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库任务中断"))?
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败"))?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn admin_count(State(state): State<ProbeState>, headers: HeaderMap) -> ApiResult {
    if headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        != Some(state.admin_token.as_ref())
    {
        return Err(api_error(StatusCode::UNAUTHORIZED, "需要管理员身份"));
    }
    let (reply, receiver) = oneshot::channel();
    state
        .database
        .sender
        .try_send(DbCommand::Count(reply))
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库队列不可用"))?;
    let count = receiver
        .await
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库任务中断"))?
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败"))?;
    Ok((StatusCode::OK, Json(count)))
}

struct ProbeServer {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<io::Result<()>>>,
    database: Arc<DbWorker>,
}

impl ProbeServer {
    async fn start(directory: &Path, state: ProbeState) -> ProbeResult<Self> {
        let router = Router::new()
            .route("/customers", get(|| async { Html(SHELL) }))
            .route("/api/customers", post(create_customer).get(admin_count))
            .route("/admin/customers", get(admin_count))
            .nest_service("/assets", ServeDir::new(directory.join("client/assets")))
            .fallback(|| async { api_error(StatusCode::NOT_FOUND, "探针路由不存在") })
            .with_state(state.clone())
            .layer(RequestBodyLimitLayer::new(512));
        Self::serve(router, state.database).await
    }

    async fn serve(router: Router, database: Arc<DbWorker>) -> ProbeResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = receiver.await;
                })
                .await
        });
        Ok(Self {
            address,
            shutdown: Some(shutdown),
            task: Some(task),
            database,
        })
    }

    async fn finish(self) -> ProbeResult {
        self.finish_with_deadline(Duration::from_secs(5)).await
    }

    async fn finish_with_deadline(mut self, grace: Duration) -> ProbeResult {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(mut task) = self.task.take() {
            let _abort_on_drop = AbortServerOnDrop(task.abort_handle());
            match tokio::time::timeout(grace, &mut task).await {
                Ok(result) => result??,
                Err(_) => {
                    task.abort();
                    let reaped = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
                    let outcome = if reaped.is_ok() {
                        "已中止并回收服务任务"
                    } else {
                        "已请求中止但任务回收超过1秒"
                    };
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("Axum 关闭超过{grace:?}，{outcome}"),
                    )
                    .into());
                }
            }
        }
        self.database.stop();
        Ok(())
    }
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

impl Drop for ProbeServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.database.stop();
    }
}

struct HttpResponse {
    status: u16,
    headers: String,
    body: String,
}

async fn request(
    address: SocketAddr,
    method: &str,
    target: &str,
    body: &str,
    authorization: Option<&str>,
) -> ProbeResult<HttpResponse> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(address).await?;
        let authorization = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        let head = format!(
            "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{authorization}\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body.as_bytes()).await?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await?;
        let response = String::from_utf8(bytes)?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .ok_or_else(|| io::Error::other("HTTP 响应缺少头部边界"))?;
        let status = headers
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| io::Error::other("HTTP 响应缺少状态码"))?
            .parse()?;
        Ok(HttpResponse {
            status,
            headers: headers.to_owned(),
            body: body.to_owned(),
        })
    })
    .await?
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
