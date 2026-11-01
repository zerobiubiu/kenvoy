//! 同一迁移事务内的数据预检和 DDL 执行。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::domain::{
    model::{ApplicationIR, FieldDefinition, FieldKind},
    validation::validate_field,
};

use super::super::schema::{
    SchemaModel,
    ddl::{column_sql, create_index, create_indexes, create_table},
    quote_identifier, quoted,
};
use super::state::table_exists;
use super::{MigrationError, Result};

pub(super) fn execute_changes(
    connection: &Connection,
    previous: &SchemaModel,
    target: &SchemaModel,
) -> Result<()> {
    for (name, table) in &target.tables {
        match previous.tables.get(name) {
            None => {
                connection.execute_batch(&create_table(name, table))?;
                create_indexes(connection, name, table)?;
            }
            Some(old) => {
                let rebuild = old
                    .columns
                    .iter()
                    .any(|(name, column)| table.columns.get(name) != Some(column));
                if rebuild {
                    let temporary = format!("_kenvoy_rebuild_{name}");
                    if table_exists(connection, &temporary)? {
                        return Err(MigrationError::Drift("发现迁移临时表，拒绝覆盖".into()));
                    }
                    let sequence: i64 = connection
                        .query_row(
                            "SELECT seq FROM sqlite_sequence WHERE name=?1",
                            [name],
                            |row| row.get(0),
                        )
                        .optional()?
                        .unwrap_or(0);
                    connection.execute_batch(&create_table(&temporary, table))?;
                    let mut common = vec![
                        "id".to_owned(),
                        "created_at".to_owned(),
                        "updated_at".to_owned(),
                    ];
                    common.extend(
                        table
                            .columns
                            .keys()
                            .filter(|column| old.columns.contains_key(*column))
                            .cloned(),
                    );
                    let columns = common
                        .iter()
                        .map(|column| quoted(column))
                        .collect::<Vec<_>>()
                        .join(",");
                    connection.execute_batch(&format!(
                        "INSERT INTO {} ({columns}) SELECT {columns} FROM {};",
                        quoted(&temporary),
                        quoted(name)
                    ))?;
                    let count_old: i64 = connection.query_row(
                        &format!("SELECT count(*) FROM {}", quoted(name)),
                        [],
                        |row| row.get(0),
                    )?;
                    let count_new: i64 = connection.query_row(
                        &format!("SELECT count(*) FROM {}", quoted(&temporary)),
                        [],
                        |row| row.get(0),
                    )?;
                    if count_old != count_new {
                        return Err(MigrationError::InvalidData("重建前后记录数不一致".into()));
                    }
                    connection.execute_batch(&format!(
                        "DROP TABLE {}; ALTER TABLE {} RENAME TO {};",
                        quoted(name),
                        quoted(&temporary),
                        quoted(name)
                    ))?;
                    if connection.execute(
                        "UPDATE sqlite_sequence SET seq=max(seq,?1) WHERE name=?2",
                        params![sequence, name],
                    )? == 0
                        && sequence > 0
                    {
                        connection.execute(
                            "INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)",
                            params![name, sequence],
                        )?;
                    }
                    create_indexes(connection, name, table)?;
                } else {
                    for (column, definition) in &table.columns {
                        if !old.columns.contains_key(column) {
                            connection.execute_batch(&format!(
                                "ALTER TABLE {} ADD COLUMN {};",
                                quoted(name),
                                column_sql(definition)
                            ))?;
                        }
                    }
                    for (index, definition) in &old.indexes {
                        if table.indexes.get(index) != Some(definition) {
                            connection.execute_batch(&format!("DROP INDEX {};", quoted(index)))?;
                        }
                    }
                    for (index, definition) in &table.indexes {
                        if old.indexes.get(index) != Some(definition) {
                            create_index(connection, name, index, definition)?;
                        }
                    }
                }
            }
        }
    }
    for name in previous
        .tables
        .keys()
        .filter(|name| !target.tables.contains_key(*name))
    {
        connection.execute_batch(&format!("DROP TABLE {};", quoted(name)))?;
    }
    Ok(())
}

pub(super) fn preflight_data(
    connection: &Connection,
    previous: &SchemaModel,
    application: &ApplicationIR,
) -> Result<()> {
    for resource in &application.resources {
        let Some(old) = previous.tables.get(&resource.name) else {
            continue;
        };
        let mut statement = connection.prepare(&format!(
            "SELECT * FROM {}",
            quote_identifier(&resource.name)?
        ))?;
        let columns: Vec<_> = statement
            .column_names()
            .map(|name| name.to_string())
            .collect();
        let mut rows = statement.query([])?;
        let mut unique: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            for field in &resource.fields {
                let value = if old.columns.contains_key(&field.name) {
                    let index = columns
                        .iter()
                        .position(|name| name == &field.name)
                        .ok_or_else(|| MigrationError::Drift("已有列缺失".into()))?;
                    from_sql(row.get_ref(index)?, field, old.columns[&field.name].kind)?
                } else {
                    field.default.clone().unwrap_or(Value::Null)
                };
                let value = validate_field(field, &value).map_err(|error| {
                    MigrationError::InvalidData(format!(
                        "{}.{}：{}",
                        resource.name, field.name, error.message
                    ))
                })?;
                if field.unique
                    && !value.is_null()
                    && !unique
                        .entry(&field.name)
                        .or_default()
                        .insert(value.to_string())
                {
                    return Err(MigrationError::InvalidData(format!(
                        "{}.{} 含重复值，无法新增唯一约束",
                        resource.name, field.name
                    )));
                }
            }
        }
    }
    Ok(())
}

fn from_sql(
    value: rusqlite::types::ValueRef<'_>,
    field: &FieldDefinition,
    previous_kind: FieldKind,
) -> Result<Value> {
    use rusqlite::types::ValueRef;
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) if field.kind == FieldKind::Boolean && matches!(value, 0 | 1) => {
            Value::Bool(value == 1)
        }
        ValueRef::Integer(value) => Value::Number(value.into()),
        ValueRef::Real(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| MigrationError::InvalidData("非有限 SQLite 数字".into()))?,
        ValueRef::Text(value) if previous_kind == FieldKind::File => serde_json::from_slice(value)
            .map_err(|_| {
                MigrationError::InvalidData(format!("文件字段 {} 不是有效 JSON", field.name))
            })?,
        ValueRef::Text(value) => Value::String(
            std::str::from_utf8(value)
                .map_err(|_| MigrationError::InvalidData("SQLite 文本不是有效 UTF-8".into()))?
                .into(),
        ),
        _ => {
            return Err(MigrationError::InvalidData(format!(
                "字段 {} 存在不支持的存储类型",
                field.name
            )));
        }
    })
}
