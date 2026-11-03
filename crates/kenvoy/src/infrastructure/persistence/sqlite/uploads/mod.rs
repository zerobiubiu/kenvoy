//! 附件与业务记录共用 SQLite；暂存配额、归属校验和绑定清理均在写事务内完成。

mod binding;
mod migration;
mod schema;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{
    application::resources::{NewUpload, StoredFile},
    domain::{
        model::{FieldKind, MAX_UPLOAD_SIZE, ResourceDefinition, UploadedFile},
        validation,
    },
};

use super::{DatabaseError, Result};
pub(super) use binding::{cleanup_removed_uploads, delete_record, synchronize};
pub(super) use migration::{migrate_bindings, validate_bindings};
pub(super) use schema::{ensure_schema, validate_schema};

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

fn invalid(field: &str, message: &str, message_key: &str) -> DatabaseError {
    validation::ValidationErrors(vec![validation::ValidationError::new(
        "KV4014",
        Some(field.into()),
        message,
        message_key,
    )])
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
        return Err(invalid(
            &field.name,
            "上传会话无效或已过期",
            "validation.files.session",
        ));
    }
    let size = upload.data.len() as u64;
    if size == 0 || size > MAX_UPLOAD_SIZE || size > constraints.max_size {
        return Err(validation::ValidationErrors(vec![
            validation::ValidationError::new(
                "KV4014",
                Some(field.name.clone()),
                "附件为空或超过单文件大小限制",
                "validation.files.upload_size",
            )
            .with_param("min", 1)
            .with_param("max", constraints.max_size.min(MAX_UPLOAD_SIZE)),
        ])
        .into());
    }
    let content_type =
        canonical_content_type(&upload.content_type, &upload.data).ok_or_else(|| {
            invalid(
                &field.name,
                "附件 MIME 无效、图片签名不符或图片格式不受支持",
                "validation.files.signature",
            )
        })?;
    if !validation::mime_matches_accept(content_type, &constraints.accept) {
        return Err(invalid(
            &field.name,
            "附件 MIME 不满足 accept 约束",
            "validation.files.accept",
        ));
    }
    let file = UploadedFile {
        id: random_id()?,
        name: upload.name,
        content_type: content_type.into(),
        size,
    };
    validation::validate_uploaded_file(&file).map_err(|error| {
        DatabaseError::from(validation::ValidationErrors(vec![
            error.into_validation(field.name.clone()),
        ]))
    })?;
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
