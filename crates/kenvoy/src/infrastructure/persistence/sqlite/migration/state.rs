//! 持久化 Schema 快照、迁移历史链和真实数据库结构核验。

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::domain::model::FieldKind;

use super::super::schema::{SchemaModel, hash};
#[cfg(test)]
use super::files::absolute_path;
use super::planning::plan_checksum;
use super::{MigrationError, MigrationRecord, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) struct ActualObject {
    pub(in super::super) kind: String,
    pub(in super::super) name: String,
    pub(in super::super) table: String,
    pub(in super::super) sql: Option<String>,
}

pub(super) struct State {
    pub(super) version: u64,
    pub(super) model: SchemaModel,
    pub(super) actual: Vec<ActualObject>,
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
#[cfg(test)]
pub fn history_path(path: &Path) -> Result<Vec<MigrationRecord>> {
    use rusqlite::OpenFlags;
    use std::time::Duration;

    let path = absolute_path(path)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    read_state(&connection)?;
    history(&connection)
}

pub(super) fn read_state(connection: &Connection) -> Result<State> {
    super::super::identity::validate_schema(connection)
        .map_err(|error| MigrationError::Drift(format!("认证宿主结构核验失败：{error}")))?;
    let uploads_exist = super::super::uploads::validate_schema(connection)?;
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
    if !uploads_exist
        && model.tables.values().any(|table| {
            table
                .columns
                .values()
                .any(|column| column.kind == FieldKind::File)
        })
    {
        return Err(MigrationError::Drift(
            "已登记附件字段的宿主存储表缺失".into(),
        ));
    }
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
    let mut previous = super::catalog::records::initial_legacy_hash(connection)?
        .unwrap_or(hash(&SchemaModel::default())?);
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

pub(super) fn table_exists(connection: &Connection, name: &str) -> Result<bool> {
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

pub(in super::super) fn actual_objects(connection: &Connection) -> Result<Vec<ActualObject>> {
    let catalog_owned = super::catalog::records::verify_owner(connection)?;
    let snapshot_owned = super::super::snapshot::validate(connection)?;
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
        if catalog_owned && super::catalog::records::TABLES.contains(&row.table.as_str()) {
            continue;
        }
        if snapshot_owned && row.table == super::super::snapshot::TABLE {
            continue;
        }
        // 认证和附件表具有独立宿主版本校验；不忽略未知 _kenvoy_ 表。
        if !matches!(
            row.table.as_str(),
            "_kenvoy_schema"
                | "_kenvoy_migrations"
                | "_kenvoy_users"
                | "_kenvoy_sessions"
                | "_kenvoy_auth_schema"
                | "_kenvoy_uploads"
                | "_kenvoy_upload_schema"
        ) {
            objects.push(row);
        }
    }
    Ok(objects)
}

pub(super) const METADATA_DDL: &str = "CREATE TABLE IF NOT EXISTS _kenvoy_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,model_json TEXT NOT NULL,schema_hash TEXT NOT NULL,actual_json TEXT NOT NULL,actual_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS _kenvoy_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL,from_hash TEXT NOT NULL,to_hash TEXT NOT NULL,plan_json TEXT NOT NULL,backup_path TEXT,applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));";
