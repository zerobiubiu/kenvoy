//! 资源记录的 SQLite 编解码与已注册标识符访问。

mod list;
mod records;

use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{
    Connection, Row,
    types::{Value as SqlValue, ValueRef},
};
use serde_json::{Map, Number, Value};

use crate::{
    application::resources::Record,
    domain::{
        model::{FieldKind, ResourceDefinition},
        validation,
    },
};

use super::{DatabaseError, Result};
pub(super) use list::list;
pub(super) use records::{create, delete, update};

/// 所有调用点的名称均已通过 Schema 注册检查；这里仍转义双引号作为第二层防护。
pub(super) fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn timestamp() -> String {
    let now: DateTime<Utc> = SystemTime::now().into();
    now.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn columns(resource: &ResourceDefinition) -> String {
    ["id", "created_at", "updated_at"]
        .into_iter()
        .map(identifier)
        .chain(resource.fields.iter().map(|field| identifier(&field.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn sql_value(value: &Value) -> Result<SqlValue> {
    match value {
        Value::Null => Ok(SqlValue::Null),
        Value::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
        Value::String(value) => Ok(SqlValue::Text(value.clone())),
        Value::Number(number) if number.is_i64() => {
            Ok(SqlValue::Integer(number.as_i64().ok_or_else(|| {
                DatabaseError::InvalidState("整数转换失败".into())
            })?))
        }
        Value::Number(number) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .map(SqlValue::Real)
            .ok_or_else(|| DatabaseError::InvalidState("非有限数字不能写入 SQLite".into())),
        Value::Array(_) => serde_json::to_string(value)
            .map(SqlValue::Text)
            .map_err(|_| DatabaseError::InvalidState("附件 JSON 编码失败".into())),
        _ => Err(DatabaseError::InvalidState(
            "资源字段只支持 JSON 标量或已验证附件数组".into(),
        )),
    }
}

fn decode(row: &Row<'_>, resource: &ResourceDefinition) -> Result<Record> {
    let mut result = Map::new();
    let id = row.get::<_, i64>(0)?;
    if id < 1 || id as f64 > validation::MAX_SAFE_INTEGER {
        return Err(DatabaseError::CorruptData(
            "系统 id 超出安全正整数范围".into(),
        ));
    }
    result.insert("id".into(), id.into());
    result.insert("created_at".into(), row.get::<_, String>(1)?.into());
    result.insert("updated_at".into(), row.get::<_, String>(2)?.into());
    for (offset, field) in resource.fields.iter().enumerate() {
        let raw = row.get_ref(offset + 3)?;
        let value = match (raw, field.kind) {
            (ValueRef::Null, _) => Value::Null,
            (ValueRef::Integer(value), FieldKind::Boolean) if matches!(value, 0 | 1) => {
                Value::Bool(value == 1)
            }
            (ValueRef::Text(value), FieldKind::File) => serde_json::from_slice(value)
                .map_err(|_| DatabaseError::CorruptData(field.name.clone()))?,
            (ValueRef::Integer(value), FieldKind::Number) => value.into(),
            (ValueRef::Real(value), FieldKind::Number) => Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| DatabaseError::CorruptData(field.name.clone()))?,
            (
                ValueRef::Text(value),
                FieldKind::Text
                | FieldKind::Textarea
                | FieldKind::Date
                | FieldKind::Datetime
                | FieldKind::Select,
            ) => Value::String(
                std::str::from_utf8(value)
                    .map_err(|_| DatabaseError::CorruptData(field.name.clone()))?
                    .to_owned(),
            ),
            _ => return Err(DatabaseError::CorruptData(field.name.clone())),
        };
        let normalized = validation::validate_field(field, &value)
            .map_err(|_| DatabaseError::CorruptData(field.name.clone()))?;
        result.insert(field.name.clone(), normalized);
    }
    Ok(result)
}

fn valid_id(id: i64) -> Result<()> {
    if id < 1 || id as f64 > validation::MAX_SAFE_INTEGER {
        return Err(DatabaseError::InvalidQuery("id 必须为安全正整数".into()));
    }
    Ok(())
}

pub(super) fn get(
    connection: &Connection,
    resource: &ResourceDefinition,
    id: i64,
) -> Result<Option<Record>> {
    valid_id(id)?;
    let sql = format!(
        "SELECT {} FROM {} WHERE id = ?1",
        columns(resource),
        identifier(&resource.name)
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([id])?;
    rows.next()?.map(|row| decode(row, resource)).transpose()
}
