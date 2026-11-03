//! 开发快照来源属于数据库自身；单文件复制或改名不能将它伪装成可接入旧库。
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};

use super::{
    lifecycle::sidecar,
    migration::{MigrationError, Result},
    schema::{SCHEMA_HASH_VERSION, SchemaModel, schema_hash},
};

pub(super) const TABLE: &str = "_kenvoy_dev_snapshot";
const DDL: &str = "CREATE TABLE _kenvoy_dev_snapshot(id INTEGER PRIMARY KEY CHECK(id=1),format_version INTEGER NOT NULL CHECK(format_version=1),kind TEXT NOT NULL CHECK(kind='development_snapshot'),schema_hash_version TEXT NOT NULL CHECK(schema_hash_version='sha256-schema-v1'),schema_hash TEXT NOT NULL)";

type Object = (String, String, Option<String>);
fn objects(connection: &Connection) -> Result<Vec<Object>> {
    let mut statement = connection.prepare("SELECT type,name,sql FROM sqlite_master WHERE tbl_name='_kenvoy_dev_snapshot' ORDER BY type,name")?;
    Ok(statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

pub(super) fn mark(connection: &mut Connection, model: &SchemaModel) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(DDL)?;
    transaction.execute(
        "INSERT INTO _kenvoy_dev_snapshot VALUES(1,1,'development_snapshot',?1,?2)",
        params![SCHEMA_HASH_VERSION, schema_hash(model)?],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn validate(connection: &Connection) -> Result<bool> {
    let actual = objects(connection)?;
    if actual.is_empty() {
        return Ok(false);
    }
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(DDL)?;
    if actual != objects(&expected)? {
        return Err(MigrationError::Drift(
            "开发快照来源标记的表、索引或触发器发生漂移".into(),
        ));
    }
    let records: Vec<(i64,i64,String,String,String)> = connection.prepare("SELECT id,format_version,kind,schema_hash_version,schema_hash FROM _kenvoy_dev_snapshot")?
        .query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?
        .collect::<std::result::Result<_, _>>()?;
    if records.len() != 1
        || records[0].0 != 1
        || records[0].1 != 1
        || records[0].2 != "development_snapshot"
        || records[0].3 != SCHEMA_HASH_VERSION
        || records[0].4.len() != 64
        || !records[0]
            .4
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MigrationError::Drift(
            "开发快照来源记录缺失、版本不兼容或内容损坏".into(),
        ));
    }
    Ok(true)
}

pub fn reject(path: &Path) -> Result<()> {
    if path == Path::new(":memory:") {
        return Ok(());
    }
    let marker = sidecar(path, ".kenvoy-snapshot.json");
    let exists = |path: &Path| {
        path.try_exists().map_err(|source| MigrationError::Io {
            path: path.into(),
            source,
        })
    };
    if exists(&marker)? {
        return Err(rejected());
    }
    if !exists(path)? {
        return Ok(());
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if validate(&connection)? {
        return Err(rejected());
    }
    Ok(())
}

fn rejected() -> MigrationError {
    MigrationError::Drift("开发快照不能用于正式 run、迁移或 baseline；复制或改名数据库也不会改变来源，请创建独立数据库执行完整正式链".into())
}
