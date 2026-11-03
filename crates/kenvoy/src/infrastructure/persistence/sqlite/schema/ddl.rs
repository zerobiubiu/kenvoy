//! Schema 列、表和索引的固定 SQLite DDL 生成。
//! 这些表达式构成 sqlite-schema-v1 的冻结执行语义；不兼容变更必须新增执行器版本，
//! 不得原地改变已固化迁移的类型、默认值、约束或索引语义。

use rusqlite::Connection;
use serde_json::Value;

use crate::domain::model::FieldKind;

use super::super::migration::Result;
use super::{ColumnModel, IndexModel, TableModel, quoted};

pub(in super::super) fn create_table(name: &str, table: &TableModel) -> String {
    let mut columns = vec![
        "id INTEGER PRIMARY KEY AUTOINCREMENT".into(),
        "created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))".into(),
        "updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))".into(),
    ];
    columns.extend(table.columns.values().map(column_sql));
    format!("CREATE TABLE {} ({});", quoted(name), columns.join(","))
}

pub(in super::super) fn column_sql(column: &ColumnModel) -> String {
    let name = quoted(&column.name);
    let storage = match column.kind {
        FieldKind::Number if column.integer => "INTEGER",
        FieldKind::Number => "REAL",
        FieldKind::Boolean => "INTEGER",
        _ => "TEXT",
    };
    let checks = column_checks(column, &name);
    format!(
        "{name} {storage}{}{} CHECK ({name} IS NULL OR ({}))",
        if column.required { " NOT NULL" } else { "" },
        column
            .default
            .as_ref()
            .map(|value| format!(" DEFAULT {}", sql_literal(value)))
            .unwrap_or_default(),
        checks.join(" AND ")
    )
}

/// 与建表 CHECK 共用固定规则，直接证明存量行约束，不以结构一致替代数据核验。
pub(in super::super) fn column_check(column: &ColumnModel) -> String {
    let name = quoted(&column.name);
    format!(
        "{name} IS NULL OR ({})",
        column_checks(column, &name).join(" AND ")
    )
}

fn column_checks(column: &ColumnModel, name: &str) -> Vec<String> {
    let mut checks = match column.kind {
        FieldKind::Number if column.integer => vec![
            format!("typeof({name})='integer'"),
            format!("{name} BETWEEN -9007199254740991 AND 9007199254740991"),
        ],
        FieldKind::Number => vec![
            format!("typeof({name}) IN ('integer','real')"),
            format!("abs({name})<=1.7976931348623157e308"),
        ],
        FieldKind::Boolean => vec![format!("typeof({name})='integer' AND {name} IN (0,1)")],
        FieldKind::File => vec![
            format!("typeof({name})='text'"),
            format!("CASE WHEN json_valid({name}) THEN json_type({name})='array' ELSE 0 END"),
        ],
        _ => vec![
            format!("typeof({name})='text'"),
            format!("instr({name},char(0))=0"),
        ],
    };
    if let Some(upload) = &column.upload {
        checks.push(format!(
            "CASE WHEN json_valid({name}) THEN json_array_length({name}) BETWEEN {} AND {} ELSE 0 END",
            usize::from(column.required),
            upload.max_files
        ));
    }
    if column.required
        && (matches!(column.kind, FieldKind::Text | FieldKind::Textarea)
            || (column.kind == FieldKind::Select && column.options.is_empty()))
    {
        checks.push(format!(
            "length(trim({name},{}))>0",
            sql_string(UNICODE_WHITESPACE)
        ));
    }
    if let Some(limit) = column.max_length {
        checks.push(format!("length({name})<={limit}"));
    }
    if let Some(min) = column.min {
        checks.push(format!("{name}>={min}"));
    }
    if let Some(max) = column.max {
        checks.push(format!("{name}<={max}"));
    }
    if column.kind == FieldKind::Select && !column.options.is_empty() {
        checks.push(format!(
            "{name} IN ({})",
            column
                .options
                .iter()
                .map(|value| sql_string(value))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if column.kind == FieldKind::Date {
        checks.push(format!(
            "length({name})=10 AND date({name}) IS NOT NULL AND date({name})={name}"
        ));
    }
    if column.kind == FieldKind::Datetime {
        checks.push(format!("strftime('%Y-%m-%dT%H:%M:%fZ',{name}) IS NOT NULL AND strftime('%Y-%m-%dT%H:%M:%fZ',{name})={name}"));
    }
    checks
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".into(),
        Value::Bool(value) => if *value { "1" } else { "0" }.into(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => sql_string(value),
        Value::Array(_) | Value::Object(_) => sql_string(&value.to_string()),
    }
}
pub(in super::super) fn create_indexes(
    connection: &Connection,
    table: &str,
    model: &TableModel,
) -> Result<()> {
    for (name, index) in &model.indexes {
        create_index(connection, table, name, index)?;
    }
    Ok(())
}
pub(in super::super) fn create_index(
    connection: &Connection,
    table: &str,
    name: &str,
    index: &IndexModel,
) -> Result<()> {
    connection.execute_batch(&format!(
        "CREATE {}INDEX {} ON {} ({});",
        if index.unique { "UNIQUE " } else { "" },
        quoted(name),
        quoted(table),
        quoted(&index.column)
    ))?;
    Ok(())
}

const UNICODE_WHITESPACE: &str = "\u{0009}\u{000A}\u{000B}\u{000C}\u{000D}\u{0020}\u{0085}\u{00A0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}";
