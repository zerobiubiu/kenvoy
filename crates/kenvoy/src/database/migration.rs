//! SQLite Schema 差异、漂移核验和事务迁移。所有 SQL 标识符来自受检模型。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    application::{ApplicationIR, FieldDefinition, FieldKind},
    validation::validate_field,
};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SchemaModel {
    pub tables: BTreeMap<String, TableModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableModel {
    pub columns: BTreeMap<String, ColumnModel>,
    pub indexes: BTreeMap<String, IndexModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnModel {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
    pub integer: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub max_length: Option<usize>,
    pub options: Vec<String>,
}

fn deserialize_default<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexModel {
    pub column: String,
    pub unique: bool,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ActualObject {
    kind: String,
    name: String,
    table: String,
    sql: Option<String>,
}

struct State {
    version: u64,
    model: SchemaModel,
    actual: Vec<ActualObject>,
}

/// 重新验证从产物加载的业务标识和约束，避免将伪造 IR 的名称插入 SQL。
pub fn schema_model(application: &ApplicationIR) -> Result<SchemaModel> {
    let mut tables = BTreeMap::new();
    let mut index_names = BTreeSet::new();
    for resource in &application.resources {
        validate_business_identifier(&resource.name)?;
        if resource.fields.is_empty() {
            return Err(MigrationError::InvalidSchema(format!(
                "资源 {} 至少需要一个字段",
                resource.name
            )));
        }
        let mut columns = BTreeMap::new();
        let mut indexes = BTreeMap::new();
        for field in &resource.fields {
            validate_business_identifier(&field.name)?;
            validate_constraints(field)?;
            let mut options: Vec<_> = field
                .options
                .iter()
                .map(|option| option.value.clone())
                .collect();
            options.sort();
            let default = field
                .default
                .as_ref()
                .map(|value| {
                    validate_field(field, value)
                        .map_err(|error| MigrationError::InvalidSchema(error.to_string()))
                })
                .transpose()?;
            let column = ColumnModel {
                name: field.name.clone(),
                kind: if field.kind == FieldKind::Textarea {
                    FieldKind::Text
                } else {
                    field.kind
                },
                required: field.required,
                default,
                integer: field.integer,
                min: field
                    .min
                    .map(|value| if value == 0.0 { 0.0 } else { value }),
                max: field
                    .max
                    .map(|value| if value == 0.0 { 0.0 } else { value }),
                max_length: field.max_length,
                options,
            };
            if columns.insert(field.name.clone(), column).is_some() {
                return Err(MigrationError::InvalidSchema(format!(
                    "资源 {} 包含重复字段 {}",
                    resource.name, field.name
                )));
            }
            if field.unique || field.index {
                let name = format!(
                    "{}_{}_{}",
                    if field.unique { "uq" } else { "idx" },
                    resource.name,
                    field.name
                );
                if !index_names.insert(name.clone()) {
                    return Err(MigrationError::InvalidSchema(format!(
                        "生成的索引名称 {name} 冲突"
                    )));
                }
                indexes.insert(
                    name,
                    IndexModel {
                        column: field.name.clone(),
                        unique: field.unique,
                    },
                );
            }
        }
        if tables
            .insert(resource.name.clone(), TableModel { columns, indexes })
            .is_some()
        {
            return Err(MigrationError::InvalidSchema(format!(
                "重复资源 {}",
                resource.name
            )));
        }
    }
    Ok(SchemaModel { tables })
}

fn validate_constraints(field: &FieldDefinition) -> Result<()> {
    if field.min.is_some_and(|value| !value.is_finite())
        || field.max.is_some_and(|value| !value.is_finite())
        || matches!((field.min,field.max),(Some(min),Some(max)) if min>max)
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 数值范围无效",
            field.name
        )));
    }
    if field.kind != FieldKind::Number
        && (field.integer || field.min.is_some() || field.max.is_some())
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 的数值约束与类型不符",
            field.name
        )));
    }
    if field.integer
        && field
            .min
            .unwrap_or(-crate::validation::MAX_SAFE_INTEGER)
            .max(-crate::validation::MAX_SAFE_INTEGER)
            .ceil()
            > field
                .max
                .unwrap_or(crate::validation::MAX_SAFE_INTEGER)
                .min(crate::validation::MAX_SAFE_INTEGER)
                .floor()
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 没有可用的安全整数范围",
            field.name
        )));
    }
    if field.max_length.is_some_and(|limit| limit > 1_000_000) {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 长度限制超出支持范围",
            field.name
        )));
    }
    if !matches!(field.kind, FieldKind::Text | FieldKind::Textarea) && field.max_length.is_some() {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 的长度约束与类型不符",
            field.name
        )));
    }
    if field.kind == FieldKind::Select {
        if field.options.is_empty() {
            return Err(MigrationError::InvalidSchema(format!(
                "选择字段 {} 必须有选项",
                field.name
            )));
        }
        let mut names = BTreeSet::new();
        for option in &field.options {
            if !names.insert(&option.value) || option.value.contains('\0') {
                return Err(MigrationError::InvalidSchema(format!(
                    "字段 {} 选项重复或包含 NUL",
                    field.name
                )));
            }
        }
    } else if !field.options.is_empty() {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 非 select 却包含选项",
            field.name
        )));
    }
    if let Some(value) = &field.default {
        validate_field(field, value)
            .map_err(|error| MigrationError::InvalidSchema(error.to_string()))?;
        if value.as_str().is_some_and(|value| value.contains('\0')) {
            return Err(MigrationError::InvalidSchema(
                "SQL 默认文本不能包含 NUL".into(),
            ));
        }
    }
    Ok(())
}

pub fn validate_identifier(name: &str) -> Result<()> {
    let mut chars = name.chars();
    if !chars.next().is_some_and(|first| first.is_ascii_lowercase())
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        || name.starts_with("sqlite_")
    {
        return Err(MigrationError::InvalidSchema(format!(
            "非法数据库标识符 {name}"
        )));
    }
    Ok(())
}

fn validate_business_identifier(name: &str) -> Result<()> {
    validate_identifier(name)?;
    if name.starts_with("kenvoy_")
        || matches!(
            name,
            "id" | "created_at" | "updated_at" | "actions" | "auth"
        )
    {
        return Err(MigrationError::InvalidSchema(format!(
            "{name} 是框架保留业务名称"
        )));
    }
    Ok(())
}

/// 对外只接受已验证资源/字段名，系统表由本模块的固定 SQL 管理。
pub fn quote_identifier(name: &str) -> Result<String> {
    validate_identifier(name)?;
    Ok(quoted(name))
}
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
pub fn schema_hash(model: &SchemaModel) -> Result<String> {
    hash(model)
}
fn hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(Sha256::digest(serde_json::to_vec(value)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn io(path: &Path, source: std::io::Error) -> MigrationError {
    MigrationError::Io {
        path: path.into(),
        source,
    }
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
    transaction.execute_batch(METADATA_DDL)?;
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

pub fn history(connection: &Connection) -> Result<Vec<MigrationRecord>> {
    if !table_exists(connection, "_kenvoy_migrations")? {
        return Ok(vec![]);
    }
    let mut statement = connection.prepare("SELECT version,checksum,from_hash,to_hash,applied_at,backup_path,plan_json FROM _kenvoy_migrations ORDER BY version")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut records = vec![];
    for row in rows {
        let (version, checksum, from_hash, to_hash, applied_at, backup, plan_json) = row?;
        let version =
            u64::try_from(version).map_err(|_| MigrationError::Drift("迁移版本为负数".into()))?;
        records.push(MigrationRecord {
            version,
            checksum,
            from_hash,
            to_hash,
            applied_at,
            backup: backup.map(PathBuf::from),
            plan: serde_json::from_str(&plan_json)?,
        });
    }
    Ok(records)
}

/// 读取历史不会创建数据库；同时核验版本链、快照和实际 Schema。
pub fn history_path(path: &Path) -> Result<Vec<MigrationRecord>> {
    let path = absolute_path(path)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    read_state(&connection)?;
    history(&connection)
}

fn plan(path: &Path, state: &State, target: &SchemaModel) -> Result<MigrationPlan> {
    let changes = diff(&state.model, target);
    let mut plan = MigrationPlan {
        database: path.into(),
        from_version: state.version,
        from_hash: schema_hash(&state.model)?,
        to_hash: schema_hash(target)?,
        actual_hash: hash(&state.actual)?,
        checksum: String::new(),
        destructive: changes.iter().any(SchemaChange::destructive),
        changes,
    };
    plan.checksum = plan_checksum(&plan)?;
    Ok(plan)
}

fn plan_checksum(plan: &MigrationPlan) -> Result<String> {
    let mut plan = plan.clone();
    plan.checksum.clear();
    hash(&plan)
}

fn diff(previous: &SchemaModel, target: &SchemaModel) -> Vec<SchemaChange> {
    let mut changes = vec![];
    for (name, table) in &target.tables {
        let Some(old) = previous.tables.get(name) else {
            changes.push(SchemaChange::CreateTable {
                table: name.clone(),
            });
            continue;
        };
        let added: Vec<_> = table
            .columns
            .keys()
            .filter(|name| !old.columns.contains_key(*name))
            .collect();
        let removed: Vec<_> = old
            .columns
            .keys()
            .filter(|name| !table.columns.contains_key(*name))
            .collect();
        if added.len() == 1 && removed.len() == 1 {
            let mut from = old.columns[removed[0]].clone();
            let to = &table.columns[added[0]];
            from.name = to.name.clone();
            if &from == to {
                changes.push(SchemaChange::RenameCandidate {
                    table: name.clone(),
                    from: removed[0].clone(),
                    to: added[0].clone(),
                });
            }
        }
        for column in added {
            let definition = &table.columns[column];
            changes.push(SchemaChange::AddColumn {
                table: name.clone(),
                column: column.clone(),
                safe: !definition.required
                    || definition
                        .default
                        .as_ref()
                        .is_some_and(|value| !value.is_null()),
            });
        }
        for column in removed {
            changes.push(SchemaChange::DropColumn {
                table: name.clone(),
                column: column.clone(),
            });
        }
        for (column, definition) in &table.columns {
            if old.columns.get(column).is_some_and(|old| old != definition) {
                changes.push(SchemaChange::ChangeColumn {
                    table: name.clone(),
                    column: column.clone(),
                });
            }
        }
        for (index, definition) in &table.indexes {
            if old.indexes.get(index) != Some(definition) {
                changes.push(SchemaChange::AddIndex {
                    table: name.clone(),
                    index: index.clone(),
                    unique: definition.unique,
                });
            }
        }
        for (index, definition) in &old.indexes {
            if table.indexes.get(index) != Some(definition) {
                changes.push(SchemaChange::DropIndex {
                    table: name.clone(),
                    index: index.clone(),
                });
            }
        }
    }
    for table in previous
        .tables
        .keys()
        .filter(|name| !target.tables.contains_key(*name))
    {
        changes.push(SchemaChange::DropTable {
            table: table.clone(),
        });
    }
    changes
}

fn read_state(connection: &Connection) -> Result<State> {
    let actual = actual_objects(connection)?;
    let schema_exists = table_exists(connection, "_kenvoy_schema")?;
    let migration_exists = table_exists(connection, "_kenvoy_migrations")?;
    if !schema_exists && !migration_exists {
        if !actual.is_empty() {
            return Err(MigrationError::Unmanaged(
                actual
                    .iter()
                    .map(|object| object.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
        return Ok(State {
            version: 0,
            model: SchemaModel::default(),
            actual,
        });
    }
    if !schema_exists || !migration_exists {
        return Err(MigrationError::Drift("迁移元数据表不完整".into()));
    }
    verify_metadata_schema(connection)?;
    let (version,model_json,schema_hash,actual_json,actual_hash):(i64,String,String,String,String)=connection.query_row("SELECT version,model_json,schema_hash,actual_json,actual_hash FROM _kenvoy_schema WHERE id=1",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    let version =
        u64::try_from(version).map_err(|_| MigrationError::Drift("迁移版本为负数".into()))?;
    let model: SchemaModel = serde_json::from_str(&model_json)?;
    let expected_actual: Vec<ActualObject> = serde_json::from_str(&actual_json)?;
    if hash(&model)? != schema_hash
        || hash(&expected_actual)? != actual_hash
        || expected_actual != actual
    {
        return Err(MigrationError::Drift(
            "实际表、列、索引、约束或保存快照与记录不一致".into(),
        ));
    }
    let records = history(connection)?;
    let mut previous = hash(&SchemaModel::default())?;
    for (index, record) in records.iter().enumerate() {
        if record.version != index as u64 + 1
            || record.plan.from_version != index as u64
            || record.from_hash != previous
            || record.from_hash != record.plan.from_hash
            || record.to_hash != record.plan.to_hash
            || record.checksum != record.plan.checksum
            || plan_checksum(&record.plan)? != record.checksum
        {
            return Err(MigrationError::Drift("迁移版本链或 checksum 不一致".into()));
        }
        previous = record.to_hash.clone();
    }
    if records.len() as u64 != version || previous != schema_hash {
        return Err(MigrationError::Drift(
            "迁移历史与当前 Schema 版本不一致".into(),
        ));
    }
    Ok(State {
        version,
        model,
        actual,
    })
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn verify_metadata_schema(connection: &Connection) -> Result<()> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(METADATA_DDL)?;
    fn read(connection: &Connection) -> Result<Vec<(String, String, Option<String>)>> {
        let mut statement=connection.prepare("SELECT type,name,sql FROM sqlite_master WHERE tbl_name IN ('_kenvoy_schema','_kenvoy_migrations') ORDER BY type,name")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    if read(connection)? != read(&expected)? {
        return Err(MigrationError::Drift(
            "迁移元数据表、索引或触发器被修改".into(),
        ));
    }
    Ok(())
}

fn actual_objects(connection: &Connection) -> Result<Vec<ActualObject>> {
    let mut statement=connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE name NOT GLOB 'sqlite_*' ORDER BY type,name")?;
    let rows = statement.query_map([], |row| {
        Ok(ActualObject {
            kind: row.get(0)?,
            name: row.get(1)?,
            table: row.get(2)?,
            sql: row.get(3)?,
        })
    })?;
    let mut objects = vec![];
    for row in rows {
        let row = row?;
        // 认证表具有独立宿主版本校验；不忽略未知 _kenvoy_ 表。
        if !matches!(
            row.table.as_str(),
            "_kenvoy_schema"
                | "_kenvoy_migrations"
                | "_kenvoy_users"
                | "_kenvoy_sessions"
                | "_kenvoy_auth_schema"
        ) {
            objects.push(row);
        }
    }
    Ok(objects)
}

fn execute_changes(
    connection: &Connection,
    previous: &SchemaModel,
    target: &SchemaModel,
) -> Result<()> {
    for (name, table) in &target.tables {
        match previous.tables.get(name) {
            None => {
                connection.execute_batch(&create_table(name, table))?;
                create_indexes(connection, name, table)?;
            }
            Some(old) => {
                let rebuild = old
                    .columns
                    .iter()
                    .any(|(name, column)| table.columns.get(name) != Some(column));
                if rebuild {
                    let temporary = format!("_kenvoy_rebuild_{name}");
                    if table_exists(connection, &temporary)? {
                        return Err(MigrationError::Drift("发现迁移临时表，拒绝覆盖".into()));
                    }
                    let sequence: i64 = connection
                        .query_row(
                            "SELECT seq FROM sqlite_sequence WHERE name=?1",
                            [name],
                            |row| row.get(0),
                        )
                        .optional()?
                        .unwrap_or(0);
                    connection.execute_batch(&create_table(&temporary, table))?;
                    let mut common = vec![
                        "id".to_owned(),
                        "created_at".to_owned(),
                        "updated_at".to_owned(),
                    ];
                    common.extend(
                        table
                            .columns
                            .keys()
                            .filter(|column| old.columns.contains_key(*column))
                            .cloned(),
                    );
                    let columns = common
                        .iter()
                        .map(|column| quoted(column))
                        .collect::<Vec<_>>()
                        .join(",");
                    connection.execute_batch(&format!(
                        "INSERT INTO {} ({columns}) SELECT {columns} FROM {};",
                        quoted(&temporary),
                        quoted(name)
                    ))?;
                    let count_old: i64 = connection.query_row(
                        &format!("SELECT count(*) FROM {}", quoted(name)),
                        [],
                        |row| row.get(0),
                    )?;
                    let count_new: i64 = connection.query_row(
                        &format!("SELECT count(*) FROM {}", quoted(&temporary)),
                        [],
                        |row| row.get(0),
                    )?;
                    if count_old != count_new {
                        return Err(MigrationError::InvalidData("重建前后记录数不一致".into()));
                    }
                    connection.execute_batch(&format!(
                        "DROP TABLE {}; ALTER TABLE {} RENAME TO {};",
                        quoted(name),
                        quoted(&temporary),
                        quoted(name)
                    ))?;
                    if connection.execute(
                        "UPDATE sqlite_sequence SET seq=max(seq,?1) WHERE name=?2",
                        params![sequence, name],
                    )? == 0
                        && sequence > 0
                    {
                        connection.execute(
                            "INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)",
                            params![name, sequence],
                        )?;
                    }
                    create_indexes(connection, name, table)?;
                } else {
                    for (column, definition) in &table.columns {
                        if !old.columns.contains_key(column) {
                            connection.execute_batch(&format!(
                                "ALTER TABLE {} ADD COLUMN {};",
                                quoted(name),
                                column_sql(definition)
                            ))?;
                        }
                    }
                    for (index, definition) in &old.indexes {
                        if table.indexes.get(index) != Some(definition) {
                            connection.execute_batch(&format!("DROP INDEX {};", quoted(index)))?;
                        }
                    }
                    for (index, definition) in &table.indexes {
                        if old.indexes.get(index) != Some(definition) {
                            create_index(connection, name, index, definition)?;
                        }
                    }
                }
            }
        }
    }
    for name in previous
        .tables
        .keys()
        .filter(|name| !target.tables.contains_key(*name))
    {
        connection.execute_batch(&format!("DROP TABLE {};", quoted(name)))?;
    }
    Ok(())
}

fn create_table(name: &str, table: &TableModel) -> String {
    let mut columns = vec![
        "id INTEGER PRIMARY KEY AUTOINCREMENT".into(),
        "created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))".into(),
        "updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))".into(),
    ];
    columns.extend(table.columns.values().map(column_sql));
    format!("CREATE TABLE {} ({});", quoted(name), columns.join(","))
}

fn column_sql(column: &ColumnModel) -> String {
    let name = quoted(&column.name);
    let storage = match column.kind {
        FieldKind::Number if column.integer => "INTEGER",
        FieldKind::Number => "REAL",
        FieldKind::Boolean => "INTEGER",
        _ => "TEXT",
    };
    let mut checks = match column.kind {
        FieldKind::Number if column.integer => vec![
            format!("typeof({name})='integer'"),
            format!("{name} BETWEEN -9007199254740991 AND 9007199254740991"),
        ],
        FieldKind::Number => vec![
            format!("typeof({name}) IN ('integer','real')"),
            format!("abs({name})<=1.7976931348623157e308"),
        ],
        FieldKind::Boolean => vec![format!("typeof({name})='integer' AND {name} IN (0,1)")],
        _ => vec![
            format!("typeof({name})='text'"),
            format!("instr({name},char(0))=0"),
        ],
    };
    if column.required && matches!(column.kind, FieldKind::Text | FieldKind::Textarea) {
        checks.push(format!(
            "length(trim({name},{}))>0",
            sql_string(UNICODE_WHITESPACE)
        ));
    }
    if let Some(limit) = column.max_length {
        checks.push(format!("length({name})<={limit}"));
    }
    if let Some(min) = column.min {
        checks.push(format!("{name}>={min}"));
    }
    if let Some(max) = column.max {
        checks.push(format!("{name}<={max}"));
    }
    if column.kind == FieldKind::Select {
        checks.push(format!(
            "{name} IN ({})",
            column
                .options
                .iter()
                .map(|value| sql_string(value))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if column.kind == FieldKind::Date {
        checks.push(format!(
            "length({name})=10 AND date({name}) IS NOT NULL AND date({name})={name}"
        ));
    }
    if column.kind == FieldKind::Datetime {
        checks.push(format!("strftime('%Y-%m-%dT%H:%M:%fZ',{name}) IS NOT NULL AND strftime('%Y-%m-%dT%H:%M:%fZ',{name})={name}"));
    }
    format!(
        "{name} {storage}{}{} CHECK ({name} IS NULL OR ({}))",
        if column.required { " NOT NULL" } else { "" },
        column
            .default
            .as_ref()
            .map(|value| format!(" DEFAULT {}", sql_literal(value)))
            .unwrap_or_default(),
        checks.join(" AND ")
    )
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".into(),
        Value::Bool(value) => if *value { "1" } else { "0" }.into(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => sql_string(value),
        _ => "NULL".into(),
    }
}
fn create_indexes(connection: &Connection, table: &str, model: &TableModel) -> Result<()> {
    for (name, index) in &model.indexes {
        create_index(connection, table, name, index)?;
    }
    Ok(())
}
fn create_index(
    connection: &Connection,
    table: &str,
    name: &str,
    index: &IndexModel,
) -> Result<()> {
    connection.execute_batch(&format!(
        "CREATE {}INDEX {} ON {} ({});",
        if index.unique { "UNIQUE " } else { "" },
        quoted(name),
        quoted(table),
        quoted(&index.column)
    ))?;
    Ok(())
}

fn preflight_data(
    connection: &Connection,
    previous: &SchemaModel,
    application: &ApplicationIR,
) -> Result<()> {
    for resource in &application.resources {
        let Some(old) = previous.tables.get(&resource.name) else {
            continue;
        };
        let mut statement = connection.prepare(&format!(
            "SELECT * FROM {}",
            quote_identifier(&resource.name)?
        ))?;
        let columns: Vec<_> = statement
            .column_names()
            .map(|name| name.to_string())
            .collect();
        let mut rows = statement.query([])?;
        let mut unique: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            for field in &resource.fields {
                let value = if old.columns.contains_key(&field.name) {
                    let index = columns
                        .iter()
                        .position(|name| name == &field.name)
                        .ok_or_else(|| MigrationError::Drift("已有列缺失".into()))?;
                    from_sql(row.get_ref(index)?, field)?
                } else {
                    field.default.clone().unwrap_or(Value::Null)
                };
                let value = validate_field(field, &value).map_err(|error| {
                    MigrationError::InvalidData(format!(
                        "{}.{}：{}",
                        resource.name, field.name, error.message
                    ))
                })?;
                if field.unique
                    && !value.is_null()
                    && !unique
                        .entry(&field.name)
                        .or_default()
                        .insert(value.to_string())
                {
                    return Err(MigrationError::InvalidData(format!(
                        "{}.{} 含重复值，无法新增唯一约束",
                        resource.name, field.name
                    )));
                }
            }
        }
    }
    Ok(())
}

fn from_sql(value: rusqlite::types::ValueRef<'_>, field: &FieldDefinition) -> Result<Value> {
    use rusqlite::types::ValueRef;
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) if field.kind == FieldKind::Boolean && matches!(value, 0 | 1) => {
            Value::Bool(value == 1)
        }
        ValueRef::Integer(value) => Value::Number(value.into()),
        ValueRef::Real(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| MigrationError::InvalidData("非有限 SQLite 数字".into()))?,
        ValueRef::Text(value) => Value::String(
            std::str::from_utf8(value)
                .map_err(|_| MigrationError::InvalidData("SQLite 文本不是有效 UTF-8".into()))?
                .into(),
        ),
        _ => {
            return Err(MigrationError::InvalidData(format!(
                "字段 {} 存在不支持的存储类型",
                field.name
            )));
        }
    })
}

fn backup_database(path: &Path, version: u64) -> Result<PathBuf> {
    let parent = path.parent().ok_or(MigrationError::BackupRequired)?;
    let directory = parent.join("backups");
    fs::create_dir_all(&directory).map_err(|error| io(&directory, error))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MigrationError::BackupRequired)?
        .as_nanos();
    let destination = directory.join(format!(
        "before-{version}-{stamp}-{}.sqlite",
        std::process::id()
    ));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| io(&destination, error))?;
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut output = Connection::open(&destination)?;
    let backup = rusqlite::backup::Backup::new(&source, &mut output)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() >= deadline {
            return Err(MigrationError::BackupTimeout);
        }
        match backup.step(256)? {
            rusqlite::backup::StepResult::Done => break,
            rusqlite::backup::StepResult::More => {}
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    drop(backup);
    let integrity: String = output.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(MigrationError::InvalidData("备份完整性校验未通过".into()));
    }
    Ok(destination)
}

fn connection_is_memory(path: &Path) -> bool {
    path == Path::new(":memory:")
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| io(path, error))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.exists() {
        return normalized
            .canonicalize()
            .map_err(|error| io(&normalized, error));
    }
    let mut existing = normalized.clone();
    let mut missing = vec![];
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        if !existing.pop() {
            return Err(MigrationError::InvalidSchema(
                "无法解析数据库绝对路径".into(),
            ));
        }
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| io(&existing, error))?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

const METADATA_DDL: &str = "CREATE TABLE IF NOT EXISTS _kenvoy_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,model_json TEXT NOT NULL,schema_hash TEXT NOT NULL,actual_json TEXT NOT NULL,actual_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS _kenvoy_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL,from_hash TEXT NOT NULL,to_hash TEXT NOT NULL,plan_json TEXT NOT NULL,backup_path TEXT,applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));";
const UNICODE_WHITESPACE: &str = "\u{0009}\u{000A}\u{000B}\u{000C}\u{000D}\u{0020}\u{0085}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{APPLICATION_IR_VERSION, ResourceDefinition},
        dsl::SourceLocation,
    };
    use kengine::{Position, SourceSpan};
    type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
    struct DatabaseFile {
        directory: PathBuf,
        path: PathBuf,
    }
    impl DatabaseFile {
        fn new() -> TestResult<Self> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("kenvoy-migration-{}-{stamp}", std::process::id()));
            fs::create_dir(&directory)?;
            Ok(Self {
                path: directory.join("app.sqlite"),
                directory,
            })
        }
        fn connect(&self) -> TestResult<Connection> {
            let connection = Connection::open(&self.path)?;
            connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
            connection.busy_timeout(Duration::from_secs(5))?;
            Ok(connection)
        }
        fn apply(
            &self,
            connection: &mut Connection,
            app: &ApplicationIR,
        ) -> Result<MigrationOutcome> {
            apply_connection(connection, app, Some(&self.path), None)
        }
        fn confirm(
            &self,
            connection: &mut Connection,
            app: &ApplicationIR,
        ) -> Result<MigrationOutcome> {
            let checksum = preview(&self.path, app)?.checksum;
            apply_connection(connection, app, Some(&self.path), Some(&checksum))
        }
    }
    impl Drop for DatabaseFile {
        fn drop(&mut self) {
            if self.directory.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.directory);
            }
        }
    }
    fn source() -> SourceLocation {
        SourceLocation {
            path: PathBuf::from("fixture.tsx"),
            span: SourceSpan {
                start: 0,
                end: 1,
                start_position: Position { line: 1, column: 1 },
                end_position: Position { line: 1, column: 2 },
            },
        }
    }
    fn field(name: &str, kind: FieldKind) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            label: name.into(),
            kind,
            required: false,
            default: None,
            integer: kind == FieldKind::Number,
            min: None,
            max: None,
            max_length: None,
            unique: false,
            index: false,
            options: vec![],
            source: source(),
        }
    }
    fn app() -> ApplicationIR {
        let mut name = field("name", FieldKind::Text);
        name.required = true;
        name.max_length = Some(100);
        ApplicationIR {
            format_version: APPLICATION_IR_VERSION,
            name: "测试".into(),
            resources: vec![ResourceDefinition {
                name: "customers".into(),
                fields: vec![
                    name,
                    field("age", FieldKind::Number),
                    field("phone", FieldKind::Text),
                ],
                source: source(),
            }],
            pages: vec![],
            actions: vec![],
            navigation: vec![],
            permissions: vec![],
            module_references: vec![],
        }
    }
    fn rows(connection: &Connection) -> TestResult<i64> {
        Ok(connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))?)
    }

    #[test]
    fn preview_and_history_do_not_create_database_or_directories() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let missing = fixture.directory.join("nested/data.sqlite");
        let plan = preview(&missing, &app())?;
        assert_eq!(plan.from_version, 0);
        assert!(!plan.destructive);
        assert_eq!(plan.changes.len(), 1);
        assert!(history_path(&missing)?.is_empty());
        assert!(!missing.parent().ok_or("缺少父目录")?.exists());
        Ok(())
    }

    #[test]
    fn initialization_persists_data_and_replay_ignores_labels_locations_and_field_order()
    -> TestResult {
        let fixture = DatabaseFile::new()?;
        let original = app();
        let expected = preview(&fixture.path, &original)?;
        let mut connection = fixture.connect()?;
        let outcome = fixture.apply(&mut connection, &original)?;
        assert!(outcome.applied);
        assert_eq!(expected.checksum, outcome.plan.checksum);
        connection.execute(
            "INSERT INTO customers(name,age) VALUES(?1,?2)",
            params!["张三", 25],
        )?;
        let timestamp: String =
            connection.query_row("SELECT created_at FROM customers", [], |row| row.get(0))?;
        assert!(timestamp.ends_with('Z'));
        drop(connection);
        let mut connection = fixture.connect()?;
        assert_eq!(rows(&connection)?, 1);
        let mut changed = original.clone();
        changed.name = "新标题".into();
        changed.resources[0].fields[0].label = "新标签".into();
        changed.resources[0].fields.reverse();
        changed.resources[0].source.path = "other.tsx".into();
        assert_eq!(
            schema_hash(&schema_model(&original)?)?,
            schema_hash(&schema_model(&changed)?)?
        );
        assert!(!fixture.apply(&mut connection, &changed)?.applied);
        assert_eq!(history(&connection)?.len(), 1);
        assert_eq!(history_path(&fixture.path)?.len(), 1);
        Ok(())
    }

    #[test]
    fn nullable_and_default_columns_and_indexes_are_safe_real_migrations() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut application = app();
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &application)?;
        connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
        let mut active = field("active", FieldKind::Boolean);
        active.required = true;
        active.default = Some(Value::Bool(true));
        active.index = true;
        application.resources[0].fields.push(active);
        application.resources[0]
            .fields
            .push(field("notes", FieldKind::Textarea));
        application.resources[0].fields[0].unique = true;
        let plan = preview(&fixture.path, &application)?;
        assert!(!plan.destructive);
        assert!(fixture.apply(&mut connection, &application)?.applied);
        let (active, notes): (i64, Option<String>) =
            connection.query_row("SELECT active,notes FROM customers", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        assert_eq!(active, 1);
        assert_eq!(notes, None);
        assert_eq!(history(&connection)?.len(), 2);
        assert!(
            connection
                .execute("INSERT INTO customers(name) VALUES('原有')", [])
                .is_err()
        );
        assert!(table_exists(&connection, "customers")?);
        Ok(())
    }

    #[test]
    fn uniqueness_preflight_and_failed_ddl_preserve_schema_history_and_data() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut application = app();
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &application)?;
        connection.execute_batch("INSERT INTO customers(name) VALUES('重复'),('重复');")?;
        application.resources[0].fields[0].unique = true;
        assert!(matches!(
            fixture.apply(&mut connection, &application),
            Err(MigrationError::InvalidData(_))
        ));
        assert_eq!(rows(&connection)?, 2);
        assert_eq!(history(&connection)?.len(), 1);
        application.resources[0].fields[0].unique = false;
        // SQLite 的 ALTER TABLE 禁止含非恒定默认表达式；通过失败的语句注入验证整个事务回滚。
        let before = actual_objects(&connection)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch("ALTER TABLE customers ADD COLUMN first TEXT;")?;
        assert!(
            transaction
                .execute_batch("CREATE UNIQUE INDEX broken_unique ON customers(name);")
                .is_err()
        );
        drop(transaction);
        assert_eq!(actual_objects(&connection)?, before);
        assert_eq!(history(&connection)?.len(), 1);
        assert!(preview(&fixture.path, &application)?.changes.is_empty());
        Ok(())
    }

    #[test]
    fn destructive_confirmation_backup_rebuild_and_sequence_high_water_are_preserved() -> TestResult
    {
        let fixture = DatabaseFile::new()?;
        let mut application = app();
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &application)?;
        connection.execute_batch("INSERT INTO customers(id,name,age,phone) VALUES(1,'保留',25,'13800000000'),(40,'已删除',30,'13900000000'); DELETE FROM customers WHERE id=40;")?;
        application.resources[0]
            .fields
            .retain(|field| field.name != "phone");
        assert!(matches!(
            fixture.apply(&mut connection, &application),
            Err(MigrationError::ConfirmationRequired { .. })
        ));
        assert_eq!(history(&connection)?.len(), 1);
        assert_eq!(rows(&connection)?, 1);
        let outcome = fixture.confirm(&mut connection, &application)?;
        let backup = outcome.backup.ok_or("缺少备份")?;
        assert!(
            backup.starts_with(fixture.directory.join("backups"))
                || backup
                    .canonicalize()?
                    .starts_with(fixture.directory.canonicalize()?.join("backups"))
        );
        let saved = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let phone: String =
            saved.query_row("SELECT phone FROM customers WHERE id=1", [], |row| {
                row.get(0)
            })?;
        assert_eq!(phone, "13800000000");
        let fields: Vec<String> = connection
            .prepare("PRAGMA table_info(customers)")?
            .query_map([], |row| row.get(1))?
            .collect::<std::result::Result<_, _>>()?;
        assert!(!fields.iter().any(|name| name == "phone"));
        connection.execute("INSERT INTO customers(name) VALUES('后来')", [])?;
        assert_eq!(connection.last_insert_rowid(), 41);
        assert_eq!(history(&connection)?.len(), 2);
        assert!(!fixture.apply(&mut connection, &application)?.applied);
        Ok(())
    }

    #[test]
    fn constraint_changes_validate_existing_values_and_roll_back_then_recover() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut application = app();
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &application)?;
        connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
        application.resources[0].fields[1].min = Some(30.0);
        assert!(matches!(
            fixture.confirm(&mut connection, &application),
            Err(MigrationError::InvalidData(_))
        ));
        assert_eq!(history(&connection)?.len(), 1);
        assert_eq!(rows(&connection)?, 1);
        application.resources[0].fields[1].min = Some(20.0);
        assert!(fixture.confirm(&mut connection, &application)?.applied);
        assert!(
            connection
                .execute("INSERT INTO customers(name,age) VALUES('不符',19)", [])
                .is_err()
        );
        assert_eq!(rows(&connection)?, 1);
        Ok(())
    }

    #[test]
    fn stale_confirmation_rename_candidates_and_unbacked_changes_are_rejected() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut application = app();
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &application)?;
        application.resources[0].fields[2].name = "mobile".into();
        let pending = preview(&fixture.path, &application)?;
        assert!(pending.changes.iter().any(|change|matches!(change,SchemaChange::RenameCandidate{from,to,..}if from=="phone"&&to=="mobile")));
        assert!(matches!(
            fixture.confirm(&mut connection, &application),
            Err(MigrationError::RenameMappingRequired)
        ));
        assert!(matches!(
            apply_connection(
                &mut connection,
                &application,
                Some(&fixture.path),
                Some("obsolete")
            ),
            Err(MigrationError::StaleConfirmation)
        ));
        let mut memory = Connection::open_in_memory()?;
        let original = app();
        apply_connection(&mut memory, &original, None, None)?;
        let mut changed = original.clone();
        changed.resources[0]
            .fields
            .retain(|field| field.name != "phone");
        let state = read_state(&memory)?;
        let checksum = plan(Path::new(":memory:"), &state, &schema_model(&changed)?)?.checksum;
        assert!(matches!(
            apply_connection(&mut memory, &changed, None, Some(&checksum)),
            Err(MigrationError::BackupRequired)
        ));
        Ok(())
    }

    #[test]
    fn schema_and_metadata_drift_and_unmanaged_objects_are_detected() -> TestResult {
        for sql in [
            "ALTER TABLE customers ADD COLUMN manual TEXT;",
            "CREATE INDEX manual_index ON customers(name);",
            "CREATE TABLE _kenvoy_unknown(value TEXT);",
            "CREATE TABLE sqlitepeople(value TEXT);",
            "UPDATE _kenvoy_migrations SET checksum='tampered';",
            "ALTER TABLE _kenvoy_schema ADD COLUMN injected TEXT;",
            "CREATE TRIGGER injected AFTER UPDATE ON _kenvoy_schema BEGIN DELETE FROM customers; END;",
        ] {
            let fixture = DatabaseFile::new()?;
            let mut connection = fixture.connect()?;
            fixture.apply(&mut connection, &app())?;
            connection.execute_batch(sql)?;
            assert!(
                matches!(
                    preview(&fixture.path, &app()),
                    Err(MigrationError::Drift(_))
                ),
                "必须检测漂移：{sql}"
            );
        }
        let fixture = DatabaseFile::new()?;
        let connection = fixture.connect()?;
        connection.execute_batch("CREATE TABLE existing(value TEXT);")?;
        assert!(matches!(
            preview(&fixture.path, &app()),
            Err(MigrationError::Unmanaged(_))
        ));
        Ok(())
    }

    #[test]
    fn model_validation_and_sql_constraints_reject_untrusted_artifacts_and_values() -> TestResult {
        let mut application = app();
        application.resources[0].name = "customers; DROP TABLE other".into();
        assert!(schema_model(&application).is_err());
        application = app();
        application.resources[0].fields[0].name = "id".into();
        assert!(schema_model(&application).is_err());
        application = app();
        application.resources[0].fields[0].default = Some(Value::Bool(true));
        assert!(schema_model(&application).is_err());
        application = app();
        application.resources[0].fields[2].min = Some(0.1);
        application.resources[0].fields[2].max = Some(0.9);
        assert!(schema_model(&application).is_err());
        application = app();
        application.resources[0].fields[0].max_length = Some(1_000_001);
        assert!(schema_model(&application).is_err());
        application = app();
        application.resources[0].fields.clear();
        assert!(schema_model(&application).is_err());
        let fixture = DatabaseFile::new()?;
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &app())?;
        for sql in [
            "INSERT INTO customers(name,age) VALUES('小数',1.2)",
            "INSERT INTO customers(name,age) VALUES('大整数',9007199254740992)",
            "INSERT INTO customers(name) VALUES('')",
            "INSERT INTO customers(name,age) VALUES('注入','1;DROP TABLE customers')",
        ] {
            assert!(connection.execute(sql, []).is_err());
        }
        assert_eq!(rows(&connection)?, 0);
        assert_eq!(quote_identifier("name")?, "\"name\"");
        assert!(quote_identifier("bad\"name").is_err());
        Ok(())
    }

    #[test]
    fn foreign_key_failure_during_destructive_ddl_rolls_back_prior_ddl_and_history() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut connection = fixture.connect()?;
        let original = app();
        fixture.apply(&mut connection, &original)?;
        connection.execute_batch("INSERT INTO customers(id,name) VALUES(1,'被引用'); CREATE TABLE _kenvoy_users(id INTEGER PRIMARY KEY, customer_id INTEGER REFERENCES customers(id)); INSERT INTO _kenvoy_users VALUES(1,1);")?;
        let mut target = original.clone();
        target.resources[0].name = "accounts".into();
        let before = actual_objects(&connection)?;
        let error = fixture
            .confirm(&mut connection, &target)
            .expect_err("FK 应阻止删除引用表");
        assert!(matches!(error, MigrationError::Sqlite(_)));
        assert_eq!(
            actual_objects(&connection)?,
            before,
            "先创建的新表必须一起回滚"
        );
        assert!(!table_exists(&connection, "accounts")?);
        assert_eq!(rows(&connection)?, 1);
        assert_eq!(history(&connection)?.len(), 1);
        assert!(preview(&fixture.path, &original)?.changes.is_empty());
        let backups = fs::read_dir(fixture.directory.join("backups"))?
            .collect::<std::io::Result<Vec<_>>>()?;
        assert_eq!(backups.len(), 1, "失败迁移也保留完整备份供核对");
        Ok(())
    }

    #[test]
    fn required_additions_type_changes_and_dropped_indexes_are_detected() -> TestResult {
        let fixture = DatabaseFile::new()?;
        let mut connection = fixture.connect()?;
        let mut original = app();
        original.resources[0].fields[2].index = true;
        fixture.apply(&mut connection, &original)?;
        connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
        let mut target = original.clone();
        let mut required = field("extra", FieldKind::Text);
        required.required = true;
        target.resources[0].fields.push(required);
        assert!(preview(&fixture.path, &target)?.destructive);
        assert!(matches!(
            fixture.confirm(&mut connection, &target),
            Err(MigrationError::InvalidData(_))
        ));
        let mut target = original.clone();
        target.resources[0].fields[1].kind = FieldKind::Text;
        target.resources[0].fields[1].integer = false;
        assert!(preview(&fixture.path, &target)?.changes.iter().any(
            |change| matches!(change,SchemaChange::ChangeColumn {column,..} if column=="age")
        ));
        assert!(
            matches!(
                fixture.confirm(&mut connection, &target),
                Err(MigrationError::InvalidData(_))
            ),
            "确认不代表允许隐式类型转换"
        );
        let mut target = original.clone();
        target.resources[0].fields[2].index = false;
        assert!(preview(&fixture.path, &target)?.destructive);
        assert!(fixture.confirm(&mut connection, &target)?.applied);
        assert_eq!(rows(&connection)?, 1);
        assert_eq!(history(&connection)?.len(), 2);
        Ok(())
    }
}
