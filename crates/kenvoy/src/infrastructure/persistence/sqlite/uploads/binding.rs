//! 业务事务内核验附件所有权、绑定同步和模型删除清理。

use rusqlite::{ToSql, Transaction, params};
use serde_json::Value;

use crate::{
    application::resources::Record,
    domain::{
        model::{FieldDefinition, FieldKind, ResourceDefinition, UploadedFile},
        validation,
    },
};

use super::super::{DatabaseError, Result, migration, schema::SchemaModel};
use super::{info, invalid};

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
pub(in super::super) fn synchronize(
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

pub(in super::super) fn delete_record(
    transaction: &Transaction<'_>,
    resource: &str,
    id: i64,
) -> Result<()> {
    transaction.execute(
        "DELETE FROM _kenvoy_uploads WHERE resource=?1 AND record_id=?2",
        params![resource, id],
    )?;
    Ok(())
}

/// 已确认的资源/字段删除和改型同时清理暂存及已绑定 BLOB；失败时与业务 DDL 一起回滚。
pub(in super::super) fn cleanup_removed_uploads(
    transaction: &Transaction<'_>,
    target: &SchemaModel,
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
