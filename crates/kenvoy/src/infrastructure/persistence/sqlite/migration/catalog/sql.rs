//! SQLite 自身解析 SQL 并逐操作授权；不使用分号切分或文本黑名单代替解析。
use super::super::{MigrationError, Result};
use super::{Assertion, MigrationStep};
use rusqlite::{
    Connection,
    hooks::{AuthAction, AuthContext, Authorization},
    types::ValueRef,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Clone)]
pub(super) struct Gate(Arc<AtomicU8>);
impl Gate {
    pub(super) fn install(connection: &mut Connection, tables: BTreeSet<String>) -> Result<Self> {
        connection.execute_batch("PRAGMA writable_schema=OFF;")?;
        let enabled = Arc::new(AtomicU8::new(0));
        let check = enabled.clone();
        connection.authorizer(Some(move |context: AuthContext<'_>| {
            let mode = check.load(Ordering::Relaxed);
            if mode == 0 {
                return Authorization::Allow;
            }
            authorize(context, &tables, mode)
        }))?;
        Ok(Self(enabled))
    }
    pub(super) fn restricted<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        self.scoped(1, operation)
    }
    fn scoped<T>(&self, mode: u8, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        struct Reset<'a>(&'a AtomicU8);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(0, Ordering::Relaxed);
            }
        }
        self.0.store(mode, Ordering::Relaxed);
        let _reset = Reset(&self.0);
        operation()
    }
    pub(super) fn copy(&self, connection: &Connection, sql: &str) -> Result<()> {
        self.scoped(2, || {
            connection.prepare(sql)?.execute([])?;
            Ok(())
        })
    }
    pub(super) fn remove(connection: &mut Connection) -> Result<()> {
        connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
        Ok(())
    }
}

fn authorize(context: AuthContext<'_>, tables: &BTreeSet<String>, mode: u8) -> Authorization {
    use AuthAction::*;
    if context.database_name.is_some_and(|name| name != "main") {
        return Authorization::Deny;
    }
    let business = |name: &str| tables.contains(&name.to_ascii_lowercase());
    if mode == 2 {
        let allowed = match context.action {
            Insert { table_name } => table_name
                .strip_prefix("_kenvoy_rebuild_")
                .is_some_and(business),
            Read { table_name, .. } => {
                business(table_name)
                    || table_name
                        .strip_prefix("_kenvoy_rebuild_")
                        .is_some_and(business)
            }
            Select | Recursive => true,
            Function { function_name } => safe_function(function_name),
            _ => false,
        };
        return if allowed {
            Authorization::Allow
        } else {
            Authorization::Deny
        };
    }
    let schema = |name: &str| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "sqlite_master" | "sqlite_schema"
        )
    };
    let allowed = match context.action {
        Read { table_name, .. } => business(table_name) || schema(table_name),
        Insert { table_name } | Delete { table_name } | Update { table_name, .. } => {
            business(table_name) || schema(table_name)
        }
        CreateTable { table_name } | DropTable { table_name } => business(table_name),
        CreateIndex {
            index_name,
            table_name,
        }
        | DropIndex {
            index_name,
            table_name,
        } => business(table_name) && safe_name(index_name),
        AlterTable {
            database_name,
            table_name,
        } => database_name == "main" && business(table_name),
        Reindex { index_name } => safe_name(index_name),
        Select | Recursive => true,
        Function { function_name } => safe_function(function_name),
        _ => false,
    };
    if allowed {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}
fn safe_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.starts_with('_') && !name.starts_with("kenvoy_") && !name.starts_with("sqlite_")
}
fn safe_function(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "abs"
            | "avg"
            | "char"
            | "coalesce"
            | "concat"
            | "concat_ws"
            | "count"
            | "date"
            | "datetime"
            | "format"
            | "glob"
            | "hex"
            | "ifnull"
            | "iif"
            | "if"
            | "instr"
            | "json"
            | "json_array"
            | "json_array_length"
            | "json_extract"
            | "json_insert"
            | "json_object"
            | "json_patch"
            | "json_remove"
            | "json_replace"
            | "json_set"
            | "json_type"
            | "json_valid"
            | "julianday"
            | "length"
            | "like"
            | "likelihood"
            | "likely"
            | "lower"
            | "ltrim"
            | "max"
            | "min"
            | "nullif"
            | "octet_length"
            | "printf"
            | "quote"
            | "replace"
            | "round"
            | "rtrim"
            | "sign"
            | "strftime"
            | "substr"
            | "substring"
            | "sum"
            | "time"
            | "timediff"
            | "total"
            | "trim"
            | "typeof"
            | "unhex"
            | "unicode"
            | "unixepoch"
            | "unlikely"
            | "upper"
            | "sqlite_rename_column"
            | "sqlite_rename_table"
            | "sqlite_rename_test"
            | "sqlite_drop_column"
            | "sqlite_rename_quotefix"
    )
}

pub(super) fn batch(connection: &Connection, gate: &Gate, sql: &str) -> Result<()> {
    gate.restricted(|| {
        connection.execute_batch(sql)?;
        Ok(())
    })
}
pub(super) fn expression(
    connection: &Connection,
    gate: &Gate,
    table: &str,
    expression: &str,
) -> Result<()> {
    gate.restricted(|| {
        let sql = format!(
            "SELECT ({expression}) FROM {} LIMIT 0",
            super::super::super::schema::quoted(table)
        );
        // rusqlite 0.41 的 prepare 校验完整尾部，非空第二语句返回 MultipleStatement。
        let statement = connection.prepare(&sql)?;
        if !statement.readonly() || statement.column_count() != 1 {
            return Err(MigrationError::InvalidSchema(
                "转换规则必须是单个只读 SQL 表达式".into(),
            ));
        }
        Ok(())
    })
}
pub(super) fn assertions(
    connection: &Connection,
    gate: &Gate,
    assertions: &[Assertion],
) -> Result<()> {
    for assertion in assertions {
        gate.restricted(|| {
            let mut statement = connection.prepare(&assertion.sql)?;
            if !statement.readonly() || statement.column_count() != 1 {
                return Err(MigrationError::InvalidSchema(
                    "迁移断言必须是单列只读查询".into(),
                ));
            }
            let mut rows = statement.query([])?;
            let row = rows
                .next()?
                .ok_or_else(|| MigrationError::InvalidData("迁移断言没有返回行".into()))?;
            let value = scalar(row.get_ref(0)?)?;
            if value != assertion.expected || rows.next()?.is_some() {
                return Err(MigrationError::InvalidData(format!(
                    "迁移断言失败：{}",
                    assertion.description
                )));
            }
            Ok(())
        })?;
    }
    Ok(())
}
fn scalar(value: ValueRef<'_>) -> Result<Value> {
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::from(value),
        ValueRef::Real(value) => Value::from(value),
        ValueRef::Text(value) => Value::String(
            std::str::from_utf8(value)
                .map_err(|_| MigrationError::InvalidData("断言返回非 UTF-8 文本".into()))?
                .into(),
        ),
        ValueRef::Blob(_) => return Err(MigrationError::InvalidData("断言不能返回 BLOB".into())),
    })
}
pub(super) fn tables(
    previous: &super::super::super::schema::SchemaModel,
    step: &MigrationStep,
) -> BTreeSet<String> {
    previous
        .tables
        .keys()
        .chain(step.schema.tables.keys())
        .cloned()
        .collect()
}
