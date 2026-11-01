use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use rusqlite::{Connection, InterruptHandle};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::application::{ApplicationIR, ResourceDefinition};

use super::{DatabaseError, ListQuery, ListResult, Record, Result, migration, query};

const QUEUE_CAPACITY: usize = 64;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseInfo {
    pub path: PathBuf,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub foreign_keys: bool,
    pub busy_timeout_ms: u32,
    pub resources: usize,
    pub generation: u64,
}

struct WorkerState {
    connection: Connection,
    path: PathBuf,
    application: ApplicationIR,
    generation: u64,
}

impl WorkerState {
    fn resource(&self, name: &str) -> Result<&ResourceDefinition> {
        self.application
            .resource(name)
            .ok_or_else(|| DatabaseError::UnknownResource(name.into()))
    }
    fn info(&self) -> Result<DatabaseInfo> {
        Ok(DatabaseInfo {
            path: self.path.clone(),
            sqlite_version: rusqlite::version().into(),
            journal_mode: self
                .connection
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))?,
            foreign_keys: self
                .connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))?
                == 1,
            busy_timeout_ms: self
                .connection
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?,
            resources: self.application.resources.len(),
            generation: self.generation,
        })
    }
}

type Job = Box<dyn FnOnce(&mut WorkerState, bool) + Send>;
enum Command {
    Run(Job),
    Wake,
}

struct Shared {
    sender: mpsc::Sender<Command>,
    closing: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
    interrupt: InterruptHandle,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        self.abort.store(true, Ordering::Release);
        self.interrupt.interrupt();
        let _ = self.sender.try_send(Command::Wake);
        // Drop 不能异步等待。正常生命周期必须显式 shutdown 并检查结果；这里只做最后兜底。
        if let Ok(slot) = self.thread.get_mut()
            && slot.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(handle) = slot.take()
        {
            let _ = handle.join();
        }
    }
}

/// 可克隆的受控句柄。Connection 从未离开专属线程；每个命令只访问已登记资源。
#[derive(Clone)]
pub struct Database {
    shared: Arc<Shared>,
}

impl Database {
    pub(crate) async fn auth_initialize(&self) -> Result<()> {
        self.submit(|state| super::auth::initialize(&mut state.connection))
            .await
    }
    pub(crate) async fn auth_add_user(
        &self,
        username: String,
        password_hash: String,
        role: crate::application::Role,
        initial: bool,
        now: i64,
    ) -> Result<crate::auth::UserView> {
        self.submit(move |state| {
            super::auth::add_user(
                &mut state.connection,
                &username,
                &password_hash,
                role,
                initial,
                now,
            )
        })
        .await
    }
    pub(crate) async fn auth_find_user(
        &self,
        username: String,
    ) -> Result<Option<super::auth::StoredUser>> {
        self.submit(move |state| super::auth::find_user(&state.connection, &username))
            .await
    }
    pub(crate) async fn auth_lookup(
        &self,
        hash: String,
        now: i64,
    ) -> Result<Option<super::auth::StoredSession>> {
        self.submit(move |state| super::auth::lookup(&state.connection, &hash, now))
            .await
    }
    pub(crate) async fn auth_rotate(
        &self,
        old_hash: Option<String>,
        new_hash: String,
        user_id: Option<i64>,
        now: i64,
        expires_at: i64,
    ) -> Result<super::auth::StoredSession> {
        self.submit(move |state| {
            super::auth::rotate(
                &mut state.connection,
                old_hash.as_deref(),
                &new_hash,
                user_id,
                now,
                expires_at,
            )
        })
        .await
    }

    pub async fn open(path: PathBuf, application: ApplicationIR) -> Result<Self> {
        Self::open_confirmed(path, application, None).await
    }

    pub async fn open_confirmed(
        path: PathBuf,
        application: ApplicationIR,
        confirmation: Option<String>,
    ) -> Result<Self> {
        // 即使调用方从生产产物反序列化 ApplicationIR，也重新检查 Schema 名称和约束。
        migration::schema_model(&application)?;
        let (sender, mut receiver) = mpsc::channel(QUEUE_CAPACITY);
        let (ready_sender, ready) = oneshot::channel();
        let closing = Arc::new(AtomicBool::new(false));
        let abort = Arc::new(AtomicBool::new(false));
        let worker_closing = closing.clone();
        let worker_abort = abort.clone();
        let thread = thread::Builder::new()
            .name("kenvoy-sqlite".into())
            .spawn(move || {
                let initialize = || -> Result<WorkerState> {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(|source| DatabaseError::Io {
                            path: parent.into(),
                            source,
                        })?;
                    }
                    let mut connection = Connection::open(&path)?;
                    connection.busy_timeout(Duration::from_secs(5))?;
                    connection.pragma_update(None, "foreign_keys", true)?;
                    connection.pragma_update(None, "journal_mode", "WAL")?;
                    connection.pragma_update(None, "synchronous", "NORMAL")?;
                    migration::apply_connection(
                        &mut connection,
                        &application,
                        Some(&path),
                        confirmation.as_deref(),
                    )?;
                    Ok(WorkerState {
                        connection,
                        path,
                        application,
                        generation: 1,
                    })
                };
                let mut state = match initialize() {
                    Ok(state) => state,
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                        return;
                    }
                };
                let interrupt = state.connection.get_interrupt_handle();
                if ready_sender.send(Ok(interrupt)).is_err() {
                    return;
                }
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Run(job) => job(&mut state, worker_abort.load(Ordering::Acquire)),
                        Command::Wake => {}
                    }
                    if worker_closing.load(Ordering::Acquire) && receiver.is_empty() {
                        break;
                    }
                }
                // Connection 在此线程析构。SQLite 自身维护 WAL，一般关闭不执行会阻塞读者的强制截断。
            })
            .map_err(|source| DatabaseError::Io {
                path: PathBuf::from("数据库工作线程"),
                source,
            })?;
        let interrupt = match ready.await {
            Ok(Ok(interrupt)) => interrupt,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(DatabaseError::WorkerLost);
            }
        };
        Ok(Self {
            shared: Arc::new(Shared {
                sender,
                closing,
                abort,
                interrupt,
                thread: Mutex::new(Some(thread)),
            }),
        })
    }

    async fn submit<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut WorkerState) -> Result<T> + Send + 'static,
    {
        if self.shared.closing.load(Ordering::Acquire) {
            return Err(DatabaseError::Closed);
        }
        let (reply, response) = oneshot::channel();
        let command = Command::Run(Box::new(move |state, cancelled| {
            let result = if cancelled {
                Err(DatabaseError::Closed)
            } else {
                operation(state)
            };
            let _ = reply.send(result);
        }));
        self.shared
            .sender
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => DatabaseError::Busy,
                mpsc::error::TrySendError::Closed(_) => DatabaseError::Closed,
            })?;
        // 不对已接受的写入施加“伪取消”超时；中断连接或丢失响应不代表写入未提交。
        response.await.map_err(|_| DatabaseError::WorkerLost)?
    }

    pub async fn info(&self) -> Result<DatabaseInfo> {
        self.submit(|state| state.info()).await
    }

    pub async fn create(&self, resource: &str, input: Value) -> Result<Record> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::create(&mut state.connection, &definition, &input)
        })
        .await
    }

    pub async fn get(&self, resource: &str, id: i64) -> Result<Option<Record>> {
        let resource = resource.to_owned();
        self.submit(move |state| query::get(&state.connection, state.resource(&resource)?, id))
            .await
    }

    pub async fn list(&self, resource: &str, query: ListQuery) -> Result<ListResult> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::list(&mut state.connection, &definition, &query)
        })
        .await
    }

    pub async fn update(&self, resource: &str, id: i64, patch: Value) -> Result<Option<Record>> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::update(&mut state.connection, &definition, id, &patch)
        })
        .await
    }

    pub async fn delete(&self, resource: &str, id: i64) -> Result<bool> {
        let resource = resource.to_owned();
        self.submit(move |state| {
            let definition = state.resource(&resource)?.clone();
            query::delete(&mut state.connection, &definition, id)
        })
        .await
    }

    /// 写队列内串行执行迁移；迁移失败时保留旧 ApplicationIR。
    /// HTTP/前端代际发布由宿主在调用前后另设屏障，不能仅依赖本方法。
    pub async fn replace_application(
        &self,
        application: ApplicationIR,
        confirmation: Option<String>,
    ) -> Result<migration::MigrationOutcome> {
        self.submit(move |state| {
            migration::schema_model(&application)?;
            let generation = state
                .generation
                .checked_add(1)
                .ok_or_else(|| DatabaseError::InvalidState("数据库代际编号已耗尽".into()))?;
            let outcome = migration::apply_connection(
                &mut state.connection,
                &application,
                Some(&state.path),
                confirmation.as_deref(),
            )?;
            state.application = application;
            state.generation = generation;
            Ok(outcome)
        })
        .await
    }

    /// 拒绝新请求，先等待已接受操作；超时后中断 SQL 并取消未执行队列，再有界回收。
    /// 返回 ShutdownTimeout 时仍保留线程句柄，调用方不得宣称连接已经释放。
    pub async fn shutdown(&self) -> Result<()> {
        self.shared.closing.store(true, Ordering::Release);
        let _ = self.shared.sender.try_send(Command::Wake);
        if tokio::time::timeout(SHUTDOWN_GRACE, self.wait_finished())
            .await
            .is_err()
        {
            self.shared.abort.store(true, Ordering::Release);
            self.shared.interrupt.interrupt();
            let _ = self.shared.sender.try_send(Command::Wake);
            if tokio::time::timeout(SHUTDOWN_GRACE, self.wait_finished())
                .await
                .is_err()
            {
                return Err(DatabaseError::ShutdownTimeout);
            }
        }
        let mut slot = self
            .shared
            .thread
            .lock()
            .map_err(|_| DatabaseError::InvalidState("线程句柄锁损坏".into()))?;
        if let Some(handle) = slot.take() {
            handle.join().map_err(|_| DatabaseError::WorkerPanicked)?;
        }
        Ok(())
    }

    async fn wait_finished(&self) {
        loop {
            let finished = self
                .shared
                .thread
                .lock()
                .map(|slot| slot.as_ref().is_none_or(JoinHandle::is_finished))
                .unwrap_or(true);
            if finished {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
