//! P03 的独占临时目录、固定迁移与 SQLite 工作线程。

use super::ProbeResult;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

type DbReply = oneshot::Sender<Result<Value, String>>;
const SCHEMA_CHECKSUM: &str = "p03-fixed-schema-v1";
static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct ScratchDirectory(PathBuf);

impl ScratchDirectory {
    pub(super) fn new() -> io::Result<Self> {
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

    pub(super) fn path(&self) -> &Path {
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

pub(super) fn open_database(path: &Path, timeout: Duration) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.busy_timeout(timeout)?;
    Ok(connection)
}

pub(super) fn apply_fixed_migration(connection: &mut Connection) -> rusqlite::Result<()> {
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

pub(super) fn count_customers(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))
}

pub(super) enum DbCommand {
    Create {
        name: String,
        age: Option<i64>,
        reply: DbReply,
    },
    Count(DbReply),
    Stop,
}

pub(super) struct DbWorker {
    pub(super) sender: SyncSender<DbCommand>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl DbWorker {
    pub(super) fn start(path: PathBuf) -> ProbeResult<Self> {
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

    pub(super) fn stop(&self) {
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
