//! SQLite 宿主运行时；连接仅由专属线程持有，资源名只从已验证 Application IR 获取。

mod identity;
pub mod lifecycle;
pub mod migration;
mod options;
mod query;
mod repositories;
pub mod schema;
pub mod snapshot;
mod uploads;
mod worker;

use std::path::PathBuf;

pub use worker::Database;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Auth(#[from] identity::StoreError),
    #[error("数据库文件 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("SQLite 操作失败：{0}")]
    Sqlite(#[source] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] migration::MigrationError),
    #[error(transparent)]
    Validation(#[from] crate::domain::validation::ValidationErrors),
    #[error("未注册资源：{0}")]
    UnknownResource(String),
    #[error("查询参数无效：{0}")]
    InvalidQuery(String),
    #[error("记录违反数据库约束，操作已回滚")]
    Conflict,
    #[error("暂存附件数量或总大小已达限额，请移除未提交附件后重试")]
    UploadLimit,
    #[error("数据库工作队列已满，请稍后再试；本次请求尚未执行")]
    Busy,
    #[error("数据库工作线程已经关闭，本次请求未执行")]
    Closed,
    #[error("数据库工作线程异常退出，操作结果需要核对")]
    WorkerLost,
    #[error("数据库关闭超过期限，不能确认连接已经释放")]
    ShutdownTimeout,
    #[error("数据库工作线程发生异常")]
    WorkerPanicked,
    #[error("数据库记录与声明类型不一致：{0}")]
    CorruptData(String),
    #[error("数据库运行状态无效：{0}")]
    InvalidState(String),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(error: rusqlite::Error) -> Self {
        if matches!(&error, rusqlite::Error::SqliteFailure(failure, _) if failure.code == rusqlite::ErrorCode::ConstraintViolation)
        {
            Self::Conflict
        } else {
            Self::Sqlite(error)
        }
    }
}

pub type Result<T> = std::result::Result<T, DatabaseError>;
