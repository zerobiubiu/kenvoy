//! 资源写入、选项核验和附件绑定共用原有 SQL 事务。

use rusqlite::{Connection, TransactionBehavior, params_from_iter, types::Value as SqlValue};
use serde_json::Value;

use crate::{
    application::resources::Record,
    domain::{
        model::{ApplicationIR, FieldKind, ResourceDefinition},
        validation::{self, WriteMode},
    },
};

use super::super::{DatabaseError, Result, options, uploads};
use super::{get, identifier, sql_value, timestamp, valid_id};

pub(in super::super) fn create(
    connection: &mut Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    input: &Value,
    upload_owner: Option<&str>,
) -> Result<Record> {
    let values = validation::validate_record(resource, input, WriteMode::Create)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    options::validate_record(&transaction, application, resource, &values)?;
    let now = timestamp();
    let mut names = vec![identifier("created_at"), identifier("updated_at")];
    let mut params = vec![SqlValue::Text(now.clone()), SqlValue::Text(now)];
    for field in &resource.fields {
        names.push(identifier(&field.name));
        params.push(sql_value(values.get(&field.name).ok_or_else(|| {
            DatabaseError::InvalidState("验证输出缺少字段".into())
        })?)?);
    }
    let placeholders = std::iter::repeat_n("?", names.len())
        .collect::<Vec<_>>()
        .join(", ");
    transaction.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES ({placeholders})",
            identifier(&resource.name),
            names.join(", ")
        ),
        params_from_iter(params),
    )?;
    let id = transaction.last_insert_rowid();
    valid_id(id).map_err(|_| {
        DatabaseError::CorruptData("系统 id 已超出安全正整数范围，写入已回滚".into())
    })?;
    uploads::synchronize(
        &transaction,
        resource,
        id,
        None,
        &values,
        upload_owner,
        uploads::now()?,
    )?;
    let result = get(&transaction, resource, id)?
        .ok_or_else(|| DatabaseError::InvalidState("写入后未找到记录".into()))?;
    transaction.commit()?;
    Ok(result)
}

pub(in super::super) fn update(
    connection: &mut Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    id: i64,
    input: &Value,
    upload_owner: Option<&str>,
) -> Result<Option<Record>> {
    valid_id(id)?;
    let values = validation::validate_record(resource, input, WriteMode::Patch)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(existing) = get(&transaction, resource, id)? else {
        return Ok(None);
    };
    let mut merged = existing.clone();
    merged.extend(values.clone());
    options::validate_record(&transaction, application, resource, &merged)?;
    if values.is_empty() {
        return Ok(Some(existing));
    }
    let mut assignments = Vec::new();
    let mut params = Vec::new();
    for (name, value) in &values {
        assignments.push(format!("{} = ?", identifier(name)));
        params.push(sql_value(value)?);
    }
    assignments.push("updated_at = ?".into());
    params.push(SqlValue::Text(timestamp()));
    params.push(SqlValue::Integer(id));
    transaction.execute(
        &format!(
            "UPDATE {} SET {} WHERE id = ?",
            identifier(&resource.name),
            assignments.join(", ")
        ),
        params_from_iter(params),
    )?;
    uploads::synchronize(
        &transaction,
        resource,
        id,
        Some(&existing),
        &merged,
        upload_owner,
        uploads::now()?,
    )?;
    let result = get(&transaction, resource, id)?;
    transaction.commit()?;
    Ok(result)
}

pub(in super::super) fn delete(
    connection: &mut Connection,
    resource: &ResourceDefinition,
    id: i64,
) -> Result<bool> {
    valid_id(id)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        &format!("DELETE FROM {} WHERE id = ?1", identifier(&resource.name)),
        [id],
    )?;
    if changed == 1
        && resource
            .fields
            .iter()
            .any(|field| field.kind == FieldKind::File)
    {
        uploads::delete_record(&transaction, &resource.name, id)?;
    }
    transaction.commit()?;
    Ok(changed == 1)
}
