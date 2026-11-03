//! 正式交付位置与 baseline 来源独立保存；旧迁移历史仅记录真实执行。
use super::super::super::schema::{SchemaModel, hash, schema_hash};
use super::super::{
    MigrationError, MigrationRecord, Result, planning,
    state::{self, ActualObject, State},
};
use super::{MigrationStep, model::require};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const OWNER: &str = "kenvoy.catalog";
pub(in super::super) const TABLES: [&str; 3] = [
    "_kenvoy_catalog_owner",
    "_kenvoy_catalog_applied",
    "_kenvoy_catalog_baseline",
];
const DDL: &str = "CREATE TABLE _kenvoy_catalog_owner(id INTEGER PRIMARY KEY CHECK(id=1),owner TEXT NOT NULL,format INTEGER NOT NULL,ddl_hash TEXT NOT NULL); CREATE TABLE _kenvoy_catalog_applied(ordinal INTEGER PRIMARY KEY,record_json TEXT NOT NULL,record_hash TEXT NOT NULL); CREATE TABLE _kenvoy_catalog_baseline(id INTEGER PRIMARY KEY CHECK(id=1),record_json TEXT NOT NULL,record_hash TEXT NOT NULL);";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Applied {
    pub ordinal: usize,
    pub id: String,
    pub checksum: String,
    pub from_hash: String,
    pub to_hash: String,
    pub legacy_version: u64,
    pub backup: Option<PathBuf>,
    pub applied_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Baseline {
    pub at_id: String,
    pub at_checksum: String,
    pub ordinal: usize,
    pub schema_hash: String,
    pub skipped_checksums: Vec<String>,
    pub source_schema: SchemaModel,
    pub legacy_history: Vec<MigrationRecord>,
    pub actual_objects: Vec<ActualObject>,
    pub proofs: Value,
    pub checksum: String,
    pub backup: Option<PathBuf>,
    pub created_at: String,
}
impl Baseline {
    pub(super) fn checksum(&self) -> Result<String> {
        hash(&(
            "kenvoy-baseline-v1",
            &self.at_id,
            &self.at_checksum,
            self.ordinal,
            &self.schema_hash,
            &self.skipped_checksums,
            &self.source_schema,
            &self.legacy_history,
            &self.actual_objects,
            &self.proofs,
        ))
    }
}
#[derive(Default)]
pub(super) struct Records {
    pub applied: Vec<Applied>,
    pub baseline: Option<Baseline>,
}

type SchemaObject = (String, String, String, Option<String>);

fn snapshot(connection: &Connection) -> Result<Vec<SchemaObject>> {
    let mut statement = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE tbl_name IN ('_kenvoy_catalog_owner','_kenvoy_catalog_applied','_kenvoy_catalog_baseline') ORDER BY type,name")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
pub(in super::super) fn verify_owner(connection: &Connection) -> Result<bool> {
    let actual = snapshot(connection)?;
    if actual.is_empty() {
        return Ok(false);
    }
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(DDL)?;
    require(
        actual == snapshot(&expected)?,
        "正式迁移内部表、索引或触发器被改写，或所有权表不完整",
    )?;
    let rows: i64 =
        connection.query_row("SELECT count(*) FROM _kenvoy_catalog_owner", [], |row| {
            row.get(0)
        })?;
    let owner: (String, i64, String) = connection.query_row(
        "SELECT owner,format,ddl_hash FROM _kenvoy_catalog_owner WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    require(
        rows == 1 && owner == (OWNER.into(), 1, hash(&DDL)?),
        "正式迁移内部表 owner、格式或 DDL 校验不一致",
    )?;
    Ok(true)
}
pub(super) fn ensure(connection: &Connection) -> Result<()> {
    if !verify_owner(connection)? {
        connection.execute_batch(DDL)?;
        connection.execute(
            "INSERT INTO _kenvoy_catalog_owner(id,owner,format,ddl_hash) VALUES(1,?1,1,?2)",
            params![OWNER, hash(&DDL)?],
        )?;
    }
    Ok(())
}
pub(super) fn read(connection: &Connection) -> Result<Records> {
    if !verify_owner(connection)? {
        return Ok(Records::default());
    }
    let mut statement = connection.prepare(
        "SELECT ordinal,record_json,record_hash FROM _kenvoy_catalog_applied ORDER BY ordinal",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut applied = vec![];
    for row in rows {
        let (ordinal, json, checksum) = row?;
        let record: Applied = serde_json::from_str(&json)?;
        require(
            i64::try_from(record.ordinal).ok() == Some(ordinal) && hash(&record)? == checksum,
            "正式迁移成功记录被改写",
        )?;
        applied.push(record);
    }
    let rows: i64 =
        connection.query_row("SELECT count(*) FROM _kenvoy_catalog_baseline", [], |row| {
            row.get(0)
        })?;
    require(rows <= 1, "baseline 来源记录重复")?;
    let baseline = connection
        .query_row(
            "SELECT record_json,record_hash FROM _kenvoy_catalog_baseline WHERE id=1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let baseline = match baseline {
        Some((json, checksum)) => {
            let record: Baseline = serde_json::from_str(&json)?;
            require(
                hash(&record)? == checksum
                    && record.checksum == record.checksum()?
                    && record.schema_hash == schema_hash(&record.source_schema)?
                    && record.ordinal == record.skipped_checksums.len()
                    && record.ordinal > 0
                    && record.skipped_checksums.last() == Some(&record.at_checksum),
                "baseline 来源记录的内容校验不一致",
            )?;
            Some(record)
        }
        None => None,
    };
    Ok(Records { applied, baseline })
}

pub(super) fn append(connection: &Connection, record: &Applied) -> Result<()> {
    connection.execute(
        "INSERT INTO _kenvoy_catalog_applied(ordinal,record_json,record_hash) VALUES(?1,?2,?3)",
        params![
            i64::try_from(record.ordinal)
                .map_err(|_| MigrationError::Drift("迁移序号超出 SQLite 整数范围".into()))?,
            serde_json::to_string(record)?,
            hash(record)?
        ],
    )?;
    Ok(())
}
pub(super) fn write_baseline(connection: &Connection, record: &Baseline) -> Result<()> {
    connection.execute(
        "INSERT INTO _kenvoy_catalog_baseline(id,record_json,record_hash) VALUES(1,?1,?2)",
        params![serde_json::to_string(record)?, hash(record)?],
    )?;
    Ok(())
}
pub(in super::super) fn initial_legacy_hash(connection: &Connection) -> Result<Option<String>> {
    Ok(read(connection)?
        .baseline
        .filter(|record| record.legacy_history.is_empty())
        .map(|record| record.schema_hash))
}

pub(super) fn write_snapshot(
    connection: &Connection,
    model: &SchemaModel,
    version: u64,
) -> Result<()> {
    let version = i64::try_from(version)
        .map_err(|_| MigrationError::Drift("迁移版本超出 SQLite 整数范围".into()))?;
    let actual = state::actual_objects(connection)?;
    connection.execute_batch(state::METADATA_DDL)?;
    connection.execute("INSERT INTO _kenvoy_schema(id,version,model_json,schema_hash,actual_json,actual_hash) VALUES(1,?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,model_json=excluded.model_json,schema_hash=excluded.schema_hash,actual_json=excluded.actual_json,actual_hash=excluded.actual_hash",params![version,serde_json::to_string(model)?,schema_hash(model)?,serde_json::to_string(&actual)?,hash(&actual)?])?;
    Ok(())
}
pub(super) fn append_legacy(
    connection: &Connection,
    before: &State,
    step: &MigrationStep,
    database: &Path,
    backup: Option<&Path>,
) -> Result<u64> {
    let plan = planning::plan(database, before, &step.schema)?;
    let version = before
        .version
        .checked_add(1)
        .ok_or_else(|| MigrationError::Drift("旧历史版本溢出".into()))?;
    let sql_version = i64::try_from(version)
        .map_err(|_| MigrationError::Drift("旧历史版本超出 SQLite 整数范围".into()))?;
    write_snapshot(connection, &step.schema, version)?;
    connection.execute("INSERT INTO _kenvoy_migrations(version,checksum,from_hash,to_hash,plan_json,backup_path) VALUES(?1,?2,?3,?4,?5,?6)",params![sql_version,plan.checksum,plan.from_hash,plan.to_hash,serde_json::to_string(&plan)?,backup.map(|path| path.to_string_lossy().into_owned())])?;
    Ok(version)
}
pub(super) fn timestamp(connection: &Connection) -> Result<String> {
    Ok(
        connection.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
            row.get(0)
        })?,
    )
}
