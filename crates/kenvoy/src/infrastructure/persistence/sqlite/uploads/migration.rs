//! 正式迁移保留附件绑定；冻结文件字段的数据约束不允许悬空或挪用 BLOB。
use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::super::{
    migration::{self, MigrationError, catalog::Rename},
    schema::{SchemaModel, quoted},
};
use crate::domain::{
    model::{FieldKind, UploadedFile},
    validation,
};

pub(in super::super) fn migrate_bindings(
    transaction: &Transaction<'_>,
    previous: &SchemaModel,
    target: &SchemaModel,
    renames: &[Rename],
) -> migration::Result<()> {
    if !super::schema::validate_schema(transaction)? {
        return Ok(());
    }
    for rename in renames {
        let file = |model: &SchemaModel, field: &str| {
            model
                .tables
                .get(&rename.table)
                .and_then(|table| table.columns.get(field))
                .is_some_and(|column| column.kind == FieldKind::File)
        };
        if file(previous, &rename.from) && file(target, &rename.to) {
            transaction.execute(
                "UPDATE _kenvoy_uploads SET field=?1 WHERE resource=?2 AND field=?3",
                params![rename.to, rename.table, rename.from],
            )?;
        }
    }
    Ok(())
}

pub(in super::super) fn validate_bindings(
    connection: &Connection,
    model: &SchemaModel,
) -> migration::Result<()> {
    if !super::schema::validate_schema(connection)? {
        if model.tables.values().any(|table| {
            table
                .columns
                .values()
                .any(|column| column.kind == FieldKind::File)
        }) {
            return Err(invalid("文件字段的宿主存储缺失"));
        }
        return Ok(());
    }
    let mut reference = connection.prepare("SELECT name=?2 AND content_type=?3 AND size=?4 AND resource=?5 AND field=?6 AND record_id=?7 AND length(data)=size FROM _kenvoy_uploads WHERE id=?1")?;
    let mut referenced = 0_i64;
    for (name, table) in &model.tables {
        for (field, column) in &table.columns {
            if column.kind != FieldKind::File {
                continue;
            }
            let constraints = column
                .upload
                .as_ref()
                .ok_or_else(|| invalid("文件字段缺少冻结上传约束"))?;
            let mut query = connection.prepare(&format!(
                "SELECT id,{} FROM {}",
                quoted(field),
                quoted(name)
            ))?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                let record_id: i64 = row.get(0)?;
                let value: Option<String> = row.get(1)?;
                let files: Vec<UploadedFile> = match value {
                    Some(value) => serde_json::from_str(&value)
                        .map_err(|_| invalid("文件字段元数据不是规范数组"))?,
                    None if !column.required => Vec::new(),
                    None => return Err(invalid("必填文件字段为空")),
                };
                if files.len() > constraints.max_files || (column.required && files.is_empty()) {
                    return Err(invalid("附件数量不满足冻结目标约束"));
                }
                let mut ids = BTreeSet::new();
                for file in &files {
                    validation::validate_uploaded_file(file).map_err(invalid)?;
                    if !ids.insert(file.id.as_str())
                        || file.size > constraints.max_size
                        || !validation::mime_matches_accept(&file.content_type, &constraints.accept)
                    {
                        return Err(invalid("附件重复、大小或 MIME 不满足冻结目标约束"));
                    }
                    let size = i64::try_from(file.size)
                        .map_err(|_| invalid("附件大小超出 SQLite 整数范围"))?;
                    let matches: Option<bool> = reference
                        .query_row(
                            params![
                                file.id,
                                file.name,
                                file.content_type,
                                size,
                                name,
                                field,
                                record_id
                            ],
                            |row| row.get(0),
                        )
                        .optional()?;
                    if matches != Some(true) {
                        return Err(invalid("附件引用、实际 BLOB 与记录绑定不一致"));
                    }
                    referenced = referenced
                        .checked_add(1)
                        .ok_or_else(|| invalid("附件引用数量溢出"))?;
                }
            }
        }
    }
    let bound: i64 = connection.query_row(
        "SELECT count(*) FROM _kenvoy_uploads WHERE record_id IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if referenced != bound {
        return Err(invalid(
            "存在未被业务记录引用的附件绑定；请提供完整迁移方案",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> MigrationError {
    MigrationError::InvalidData(message.into())
}
