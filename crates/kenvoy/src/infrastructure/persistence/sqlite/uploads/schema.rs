//! 附件宿主表的固定 DDL、版本和漂移核验。

use rusqlite::Connection;

use super::super::migration;

const UPLOADS: &str = "CREATE TABLE _kenvoy_uploads(id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=64 AND id NOT GLOB '*[^0-9a-f]*'),resource TEXT NOT NULL,field TEXT NOT NULL,record_id INTEGER CHECK(record_id IS NULL OR record_id BETWEEN 1 AND 9007199254740991),owner TEXT NOT NULL CHECK(length(owner)=64 AND owner NOT GLOB '*[^0-9a-f]*'),name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 255),content_type TEXT NOT NULL,size INTEGER NOT NULL CHECK(size BETWEEN 1 AND 10485760),data BLOB NOT NULL CHECK(typeof(data)='blob' AND length(data)=size),created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL CHECK(expires_at>created_at))";
const META: &str = "CREATE TABLE _kenvoy_upload_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1))";
const EXPIRY: &str =
    "CREATE INDEX _kenvoy_uploads_expiry ON _kenvoy_uploads(expires_at) WHERE record_id IS NULL";
const OWNER: &str =
    "CREATE INDEX _kenvoy_uploads_owner ON _kenvoy_uploads(owner) WHERE record_id IS NULL";
const BINDING: &str =
    "CREATE INDEX _kenvoy_uploads_binding ON _kenvoy_uploads(resource,field,record_id)";
const DDL: [&str; 5] = [UPLOADS, META, EXPIRY, OWNER, BINDING];

type SchemaObject = (String, String, String, Option<String>);

fn snapshot(connection: &Connection) -> migration::Result<Vec<SchemaObject>> {
    let mut statement = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE tbl_name IN ('_kenvoy_uploads','_kenvoy_upload_schema') ORDER BY type,name")?;
    Ok(statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?)
}

/// 仅允许首次完全缺失；已有宿主表必须精确匹配固定 DDL、索引、触发器与版本。
pub(in super::super) fn validate_schema(connection: &Connection) -> migration::Result<bool> {
    let actual = snapshot(connection)?;
    if actual.is_empty() {
        return Ok(false);
    }
    let expected = Connection::open_in_memory()?;
    for sql in DDL {
        expected.execute_batch(sql)?;
    }
    if actual != snapshot(&expected)? {
        return Err(migration::MigrationError::Drift(
            "固定附件表、索引或触发器发生漂移".into(),
        ));
    }
    let versions: Vec<(i64, i64)> = connection
        .prepare("SELECT id,version FROM _kenvoy_upload_schema")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    if versions != [(1, 1)] {
        return Err(migration::MigrationError::Drift(
            "不支持的附件版本或版本记录损坏".into(),
        ));
    }
    Ok(true)
}

pub(in super::super) fn ensure_schema(connection: &Connection) -> migration::Result<()> {
    if !validate_schema(connection)? {
        for sql in DDL {
            connection.execute_batch(sql)?;
        }
        connection.execute(
            "INSERT INTO _kenvoy_upload_schema(id,version) VALUES(1,1)",
            [],
        )?;
    }
    Ok(())
}
