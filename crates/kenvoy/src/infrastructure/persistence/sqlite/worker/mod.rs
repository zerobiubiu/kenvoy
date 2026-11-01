mod identity;
mod resources;

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
use tokio::sync::{mpsc, oneshot};

use crate::domain::model::{ApplicationIR, ResourceDefinition};

use super::{DatabaseError, Result, migration};

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
                    super::uploads::initialize(&mut connection, super::uploads::now()?)?;
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
mod tests;
