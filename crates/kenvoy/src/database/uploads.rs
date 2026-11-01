//! 附件与业务记录共用 SQLite；暂存配额、归属校验和绑定清理均在写事务内完成。

use rusqlite::{Connection, OptionalExtension, ToSql, Transaction, TransactionBehavior, params};
use serde_json::Value;

use crate::{
    application::{FieldDefinition, FieldKind, MAX_UPLOAD_SIZE, ResourceDefinition, UploadedFile},
    validation,
};

use super::{DatabaseError, Record, Result, migration};

pub(super) fn now() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_secs()).ok())
        .ok_or_else(|| DatabaseError::InvalidState("系统时间无效".into()))
}

const SESSION_FILES: i64 = 20;
const SESSION_BYTES: i64 = 40 * 1024 * 1024;
const GLOBAL_FILES: i64 = 1024;
const GLOBAL_BYTES: i64 = 256 * 1024 * 1024;
const UPLOADS: &str = "CREATE TABLE _kenvoy_uploads(id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=64 AND id NOT GLOB '*[^0-9a-f]*'),resource TEXT NOT NULL,field TEXT NOT NULL,record_id INTEGER CHECK(record_id IS NULL OR record_id BETWEEN 1 AND 9007199254740991),owner TEXT NOT NULL CHECK(length(owner)=64 AND owner NOT GLOB '*[^0-9a-f]*'),name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 255),content_type TEXT NOT NULL,size INTEGER NOT NULL CHECK(size BETWEEN 1 AND 10485760),data BLOB NOT NULL CHECK(typeof(data)='blob' AND length(data)=size),created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL CHECK(expires_at>created_at))";
const META: &str = "CREATE TABLE _kenvoy_upload_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL CHECK(version=1))";
const EXPIRY: &str =
    "CREATE INDEX _kenvoy_uploads_expiry ON _kenvoy_uploads(expires_at) WHERE record_id IS NULL";
const OWNER: &str =
    "CREATE INDEX _kenvoy_uploads_owner ON _kenvoy_uploads(owner) WHERE record_id IS NULL";
const BINDING: &str =
    "CREATE INDEX _kenvoy_uploads_binding ON _kenvoy_uploads(resource,field,record_id)";
const DDL: [&str; 5] = [UPLOADS, META, EXPIRY, OWNER, BINDING];

pub struct NewUpload {
    pub resource: String,
    pub field: String,
    pub owner: String,
    pub name: String,
    pub content_type: String,
    pub data: Vec<u8>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub file: UploadedFile,
    pub resource: String,
    pub field: String,
    pub record_id: Option<i64>,
    pub owner: String,
    pub expires_at: i64,
}

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
pub(super) fn validate_schema(connection: &Connection) -> migration::Result<bool> {
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

pub(super) fn ensure_schema(connection: &Connection) -> migration::Result<()> {
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

pub(super) fn initialize(connection: &mut Connection, now: i64) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure_schema(&transaction)?;
    purge_expired(&transaction, now)?;
    transaction.commit()?;
    Ok(())
}

fn purge_expired(connection: &Connection, now: i64) -> Result<()> {
    connection.execute(
        "DELETE FROM _kenvoy_uploads WHERE record_id IS NULL AND expires_at<=?1",
        [now],
    )?;
    Ok(())
}

fn invalid(field: &str, message: &str) -> DatabaseError {
    validation::ValidationErrors(vec![validation::ValidationError {
        code: "KV4014".into(),
        field: Some(field.into()),
        message: message.into(),
    }])
    .into()
}

fn random_id() -> Result<String> {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)
        .map_err(|_| DatabaseError::InvalidState("无法生成附件随机标识".into()))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut id = String::with_capacity(64);
    for byte in random {
        id.push(char::from(HEX[usize::from(byte >> 4)]));
        id.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Ok(id)
}

fn image_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn canonical_content_type<'a>(declared: &'a str, data: &[u8]) -> Option<&'a str> {
    if !validation::valid_mime_type(declared) {
        return None;
    }
    match image_type(data) {
        Some(detected) if !declared.starts_with("image/") || declared == detected => Some(detected),
        Some(_) => None,
        None if declared.starts_with("image/") => None,
        None => Some(declared),
    }
}

fn quota_full(count: i64, bytes: i64, incoming: i64, files: i64, limit: i64) -> bool {
    count >= files || bytes > limit - incoming
}

pub(super) fn stage(
    connection: &mut Connection,
    resource: &ResourceDefinition,
    upload: NewUpload,
    now: i64,
) -> Result<UploadedFile> {
    let field = resource
        .field(&upload.field)
        .filter(|field| field.kind == FieldKind::File)
        .ok_or_else(|| DatabaseError::InvalidQuery("上传目标必须为已注册附件字段".into()))?;
    let constraints = field
        .upload
        .as_ref()
        .ok_or_else(|| DatabaseError::InvalidState("附件字段缺少上传约束".into()))?;
    if upload.resource != resource.name {
        return Err(DatabaseError::InvalidQuery(
            "上传资源与目标定义不一致".into(),
        ));
    }
    if !validation::valid_upload_id(&upload.owner) || upload.expires_at <= now {
        return Err(invalid(&field.name, "上传会话无效或已过期"));
    }
    let size = upload.data.len() as u64;
    if size == 0 || size > MAX_UPLOAD_SIZE || size > constraints.max_size {
        return Err(invalid(&field.name, "附件为空或超过单文件大小限制"));
    }
    let content_type =
        canonical_content_type(&upload.content_type, &upload.data).ok_or_else(|| {
            invalid(
                &field.name,
                "附件 MIME 无效、图片签名不符或图片格式不受支持",
            )
        })?;
    if !validation::mime_matches_accept(content_type, &constraints.accept) {
        return Err(invalid(&field.name, "附件 MIME 不满足 accept 约束"));
    }
    let file = UploadedFile {
        id: random_id()?,
        name: upload.name,
        content_type: content_type.into(),
        size,
    };
    validation::validate_uploaded_file(&file).map_err(|message| invalid(&field.name, message))?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    purge_expired(&transaction, now)?;
    let (count, bytes): (i64, i64) = transaction.query_row(
        "SELECT count(*),coalesce(sum(size),0) FROM _kenvoy_uploads WHERE record_id IS NULL",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if quota_full(count, bytes, size as i64, GLOBAL_FILES, GLOBAL_BYTES) {
        transaction.commit()?;
        return Err(DatabaseError::UploadLimit);
    }
    let (count, bytes): (i64, i64) = transaction.query_row(
        "SELECT count(*),coalesce(sum(size),0) FROM _kenvoy_uploads WHERE record_id IS NULL AND owner=?1",
        [&upload.owner],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if quota_full(count, bytes, size as i64, SESSION_FILES, SESSION_BYTES) {
        transaction.commit()?;
        return Err(DatabaseError::UploadLimit);
    }
    transaction.execute(
        "INSERT INTO _kenvoy_uploads(id,resource,field,owner,name,content_type,size,data,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![file.id, upload.resource, upload.field, upload.owner, file.name, file.content_type, size as i64, upload.data, now, upload.expires_at],
    )?;
    transaction.commit()?;
    Ok(file)
}

pub(super) fn info(connection: &Connection, id: &str, now: i64) -> Result<Option<StoredFile>> {
    if !validation::valid_upload_id(id) {
        return Ok(None);
    }
    let stored = connection
        .query_row(
            "SELECT id,name,content_type,size,resource,field,record_id,owner,expires_at FROM _kenvoy_uploads WHERE id=?1 AND (record_id IS NOT NULL OR expires_at>?2)",
            params![id, now],
            |row| {
                Ok(StoredFile {
                    file: UploadedFile {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        content_type: row.get(2)?,
                        size: u64::from(row.get::<_, u32>(3)?),
                    },
                    resource: row.get(4)?,
                    field: row.get(5)?,
                    record_id: row.get(6)?,
                    owner: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            },
        )
        .optional()?;
    if let Some(stored) = &stored {
        validation::validate_uploaded_file(&stored.file)
            .map_err(|_| DatabaseError::CorruptData("附件元数据无效".into()))?;
        if !validation::valid_upload_id(&stored.owner)
            || stored
                .record_id
                .is_some_and(|id| !(1..=9_007_199_254_740_991).contains(&id))
        {
            return Err(DatabaseError::CorruptData("附件绑定归属无效".into()));
        }
    }
    Ok(stored)
}

pub(super) fn content(connection: &Connection, id: &str, now: i64) -> Result<Option<Vec<u8>>> {
    if !validation::valid_upload_id(id) {
        return Ok(None);
    }
    Ok(connection
        .query_row(
            "SELECT data FROM _kenvoy_uploads WHERE id=?1 AND (record_id IS NOT NULL OR expires_at>?2)",
            params![id, now],
            |row| row.get(0),
        )
        .optional()?)
}

pub(super) fn delete_staged(connection: &Connection, id: &str, owner: &str) -> Result<bool> {
    if !validation::valid_upload_id(id) || !validation::valid_upload_id(owner) {
        return Ok(false);
    }
    Ok(connection.execute(
        "DELETE FROM _kenvoy_uploads WHERE id=?1 AND owner=?2 AND record_id IS NULL",
        params![id, owner],
    )? == 1)
}

fn file_id<'a>(field: &FieldDefinition, value: &'a Value) -> Result<&'a str> {
    value["id"]
        .as_str()
        .ok_or_else(|| invalid(&field.name, "附件引用缺少有效标识"))
}

fn metadata_matches(stored: &UploadedFile, value: &Value) -> bool {
    value["id"].as_str() == Some(stored.id.as_str())
        && value["name"].as_str() == Some(stored.name.as_str())
        && value["content_type"].as_str() == Some(stored.content_type.as_str())
        && value["size"].as_u64() == Some(stored.size)
}

/// 只接受原记录保留的绑定引用，或当前会话仍有效的暂存引用；任何失败由调用事务整体回滚。
pub(super) fn synchronize(
    transaction: &Transaction<'_>,
    resource: &ResourceDefinition,
    record_id: i64,
    previous: Option<&Record>,
    values: &Record,
    owner: Option<&str>,
    now: i64,
) -> Result<()> {
    for field in resource
        .fields
        .iter()
        .filter(|field| field.kind == FieldKind::File)
    {
        let value = values
            .get(&field.name)
            .ok_or_else(|| DatabaseError::InvalidState("附件同步缺少字段值".into()))?;
        let files = match value {
            Value::Null => &[][..],
            Value::Array(files) => files.as_slice(),
            _ => return Err(invalid(&field.name, "附件字段必须为数组或 null")),
        };
        let constraints = field
            .upload
            .as_ref()
            .ok_or_else(|| DatabaseError::InvalidState("附件字段缺少上传约束".into()))?;
        if files.len() > constraints.max_files || (field.required && files.is_empty()) {
            return Err(invalid(&field.name, "附件数量不满足字段约束"));
        }
        let mut ids = Vec::with_capacity(files.len());
        for file in files {
            let id = file_id(field, file)?;
            if ids.contains(&id) {
                return Err(invalid(&field.name, "附件引用不能重复"));
            }
            let stored = info(transaction, id, now)?
                .ok_or_else(|| invalid(&field.name, "附件不存在或暂存已经过期"))?;
            if stored.resource != resource.name
                || stored.field != field.name
                || !metadata_matches(&stored.file, file)
                || stored.file.size > constraints.max_size
                || !validation::mime_matches_accept(&stored.file.content_type, &constraints.accept)
            {
                return Err(invalid(&field.name, "附件归属、元数据或当前上传约束不匹配"));
            }
            match stored.record_id {
                Some(bound_id) => {
                    let retained = previous
                        .and_then(|record| record.get(&field.name))
                        .and_then(Value::as_array)
                        .is_some_and(|files| {
                            files.iter().any(|file| file["id"].as_str() == Some(id))
                        });
                    if bound_id != record_id || !retained {
                        return Err(invalid(&field.name, "已绑定附件不能挪用到其他记录或字段"));
                    }
                }
                None => {
                    if owner != Some(stored.owner.as_str()) {
                        return Err(invalid(&field.name, "暂存附件不属于当前上传会话"));
                    }
                    let bound = transaction.execute(
                        "UPDATE _kenvoy_uploads SET record_id=?1 WHERE id=?2 AND record_id IS NULL",
                        params![record_id, id],
                    )?;
                    if bound != 1 {
                        return Err(DatabaseError::InvalidState("附件绑定状态发生变化".into()));
                    }
                }
            }
            ids.push(id);
        }
        let mut sql = String::from(
            "DELETE FROM _kenvoy_uploads WHERE resource=? AND field=? AND record_id=?",
        );
        if !ids.is_empty() {
            sql.push_str(" AND id NOT IN (");
            for index in 0..ids.len() {
                if index != 0 {
                    sql.push(',');
                }
                sql.push('?');
            }
            sql.push(')');
        }
        let mut params: Vec<&dyn ToSql> = Vec::with_capacity(3 + ids.len());
        params.extend([&resource.name as &dyn ToSql, &field.name, &record_id]);
        params.extend(ids.iter().map(|id| id as &dyn ToSql));
        transaction.execute(&sql, params.as_slice())?;
    }
    Ok(())
}

pub(super) fn delete_record(transaction: &Transaction<'_>, resource: &str, id: i64) -> Result<()> {
    transaction.execute(
        "DELETE FROM _kenvoy_uploads WHERE resource=?1 AND record_id=?2",
        params![resource, id],
    )?;
    Ok(())
}

/// 已确认的资源/字段删除和改型同时清理暂存及已绑定 BLOB；失败时与业务 DDL 一起回滚。
pub(super) fn cleanup_removed_uploads(
    transaction: &Transaction<'_>,
    target: &migration::SchemaModel,
) -> migration::Result<()> {
    if !transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='_kenvoy_uploads')",
        [],
        |row| row.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let bindings: Vec<(String, String)> = transaction
        .prepare("SELECT DISTINCT resource,field FROM _kenvoy_uploads")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (resource, field) in bindings {
        if !target
            .tables
            .get(&resource)
            .and_then(|table| table.columns.get(&field))
            .is_some_and(|column| column.kind == FieldKind::File)
        {
            transaction.execute(
                "DELETE FROM _kenvoy_uploads WHERE resource=?1 AND field=?2",
                params![resource, field],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;
