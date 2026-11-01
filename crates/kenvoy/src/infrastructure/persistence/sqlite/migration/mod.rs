//! SQLite Schema 差异、漂移核验和事务迁移。所有 SQL 标识符来自受检模型。

mod execution;
mod files;
mod planning;
mod state;

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::domain::model::{ApplicationIR, FieldKind};

use super::schema::hash;
pub use super::schema::{SchemaModel, schema_model};
use execution::{execute_changes, preflight_data};
use files::{absolute_path, backup_database, connection_is_memory};
use planning::plan;
pub use state::history_path;
use state::{METADATA_DDL, State, actual_objects, read_state};

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("数据库访问失败：{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("迁移元数据编码失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("访问迁移文件 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Schema 定义无效：{0}")]
    InvalidSchema(String),
    #[error("数据库结构或迁移记录发生漂移：{0}")]
    Drift(String),
    #[error("数据库包含未纳入迁移管理的对象，拒绝接管：{0}")]
    Unmanaged(String),
    #[error("计划包含破坏性操作；请预览后使用完整 checksum {checksum} 明确确认")]
    ConfirmationRequired { checksum: String },
    #[error("确认的 checksum 与当前数据库和目标 Schema 不匹配，请重新预览")]
    StaleConfirmation,
    #[error("检测到候选字段改名，必须提供明确映射后才可执行；首版禁止猜测或按删除加新增执行")]
    RenameMappingRequired,
    #[error("破坏性操作必须连接持久化数据库，并先保存一致性备份")]
    BackupRequired,
    #[error("现有数据不满足迁移后约束：{0}")]
    InvalidData(String),
    #[error("一致性备份超过时间限制，迁移尚未执行")]
    BackupTimeout,
}

pub type Result<T> = std::result::Result<T, MigrationError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaChange {
    CreateTable {
        table: String,
    },
    AddColumn {
        table: String,
        column: String,
        safe: bool,
    },
    AddIndex {
        table: String,
        index: String,
        unique: bool,
    },
    DropTable {
        table: String,
    },
    DropColumn {
        table: String,
        column: String,
    },
    ChangeColumn {
        table: String,
        column: String,
    },
    DropIndex {
        table: String,
        index: String,
    },
    RenameCandidate {
        table: String,
        from: String,
        to: String,
    },
}

impl SchemaChange {
    fn destructive(&self) -> bool {
        !matches!(
            self,
            Self::CreateTable { .. } | Self::AddIndex { .. } | Self::AddColumn { safe: true, .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub database: PathBuf,
    pub from_version: u64,
    pub from_hash: String,
    pub to_hash: String,
    pub actual_hash: String,
    pub checksum: String,
    pub destructive: bool,
    pub changes: Vec<SchemaChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationOutcome {
    pub plan: MigrationPlan,
    pub applied: bool,
    pub backup: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub version: u64,
    pub checksum: String,
    pub from_hash: String,
    pub to_hash: String,
    pub applied_at: String,
    pub backup: Option<PathBuf>,
    pub plan: MigrationPlan,
}

/// 只读预览；不存在的文件和父目录都不会被创建。
pub fn preview(path: &Path, application: &ApplicationIR) -> Result<MigrationPlan> {
    let path = absolute_path(path)?;
    let target = schema_model(application)?;
    if !path.exists() {
        return plan(
            &path,
            &State {
                version: 0,
                model: SchemaModel::default(),
                actual: vec![],
            },
            &target,
        );
    }
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    plan(&path, &read_state(&connection)?, &target)
}

/// 调用方持有连接所有权并负责 WAL、foreign_keys 和 busy_timeout 配置。
pub fn apply_connection(
    connection: &mut Connection,
    application: &ApplicationIR,
    db_path: Option<&Path>,
    confirmation: Option<&str>,
) -> Result<MigrationOutcome> {
    let database = match db_path {
        Some(path) => absolute_path(path)?,
        None => PathBuf::from(":memory:"),
    };
    if let Some(actual) = connection.path()
        && !actual.is_empty()
        && db_path.is_some()
        && absolute_path(Path::new(actual))? != database
    {
        return Err(MigrationError::InvalidSchema(
            "连接与预览数据库路径不一致".into(),
        ));
    }
    let target = schema_model(application)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let state = read_state(&transaction)?;
    let plan = plan(&database, &state, &target)?;
    if confirmation.is_some_and(|confirmation| confirmation != plan.checksum) {
        return Err(MigrationError::StaleConfirmation);
    }
    if plan.changes.is_empty() {
        return Ok(MigrationOutcome {
            plan,
            applied: false,
            backup: None,
        });
    }
    if plan.destructive && confirmation.is_none() {
        return Err(MigrationError::ConfirmationRequired {
            checksum: plan.checksum,
        });
    }
    if plan
        .changes
        .iter()
        .any(|change| matches!(change, SchemaChange::RenameCandidate { .. }))
    {
        return Err(MigrationError::RenameMappingRequired);
    }
    let backup = if plan.destructive {
        if db_path.is_none() || connection_is_memory(&database) {
            return Err(MigrationError::BackupRequired);
        }
        Some(backup_database(&database, state.version + 1)?)
    } else {
        None
    };
    preflight_data(&transaction, &state.model, application)?;
    if target.tables.values().any(|table| {
        table
            .columns
            .values()
            .any(|column| column.kind == FieldKind::File)
    }) {
        super::uploads::ensure_schema(&transaction)?;
    }
    transaction.execute_batch(METADATA_DDL)?;
    super::uploads::cleanup_removed_uploads(&transaction, &target)?;
    execute_changes(&transaction, &state.model, &target)?;
    let actual = actual_objects(&transaction)?;
    let actual_json = serde_json::to_string(&actual)?;
    let version = i64::try_from(
        state
            .version
            .checked_add(1)
            .ok_or_else(|| MigrationError::Drift("迁移版本溢出".into()))?,
    )
    .map_err(|_| MigrationError::Drift("迁移版本超出SQLite整数范围".into()))?;
    transaction.execute("INSERT INTO _kenvoy_schema(id, version, model_json, schema_hash, actual_json, actual_hash) VALUES(1,?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,model_json=excluded.model_json,schema_hash=excluded.schema_hash,actual_json=excluded.actual_json,actual_hash=excluded.actual_hash", params![version, serde_json::to_string(&target)?, plan.to_hash, actual_json, hash(&actual)?])?;
    transaction.execute("INSERT INTO _kenvoy_migrations(version,checksum,from_hash,to_hash,plan_json,backup_path) VALUES(?1,?2,?3,?4,?5,?6)", params![version, plan.checksum, plan.from_hash, plan.to_hash, serde_json::to_string(&plan)?, backup.as_ref().map(|path| path.to_string_lossy().into_owned())])?;
    transaction.commit()?;
    Ok(MigrationOutcome {
        plan,
        applied: true,
        backup,
    })
}

#[cfg(test)]
mod tests;
