//! sqlite-schema-v1 的固定持久化数据约束；与现行 ApplicationIR 业务回调隔离。
use super::super::migration::{MigrationError, Result};
use super::{SchemaModel, ddl, quoted};
use rusqlite::Connection;

pub(super) fn verify(connection: &Connection, model: &SchemaModel) -> Result<()> {
    let mut statement = connection.prepare("PRAGMA integrity_check")?;
    let mut rows = statement.query([])?;
    let mut checked = false;
    while let Some(row) = rows.next()? {
        let result: String = row.get(0)?;
        if result != "ok" {
            return Err(MigrationError::InvalidData(format!(
                "SQLite 完整性或冻结约束校验失败：{result}"
            )));
        }
        checked = true;
    }
    if !checked {
        return Err(MigrationError::InvalidData(
            "SQLite 完整性检查没有返回结果".into(),
        ));
    }
    let mut foreign = connection.prepare("PRAGMA foreign_key_check")?;
    if foreign.query([])?.next()?.is_some() {
        return Err(MigrationError::InvalidData(
            "数据库含违反外键约束的记录".into(),
        ));
    }
    for (name, table) in &model.tables {
        let mut predicate = String::from(
            "typeof(id)<>'integer' OR id<1 OR id>9007199254740991 OR typeof(created_at)<>'text' OR strftime('%Y-%m-%dT%H:%M:%fZ',created_at) IS NULL OR strftime('%Y-%m-%dT%H:%M:%fZ',created_at)<>created_at OR typeof(updated_at)<>'text' OR strftime('%Y-%m-%dT%H:%M:%fZ',updated_at) IS NULL OR strftime('%Y-%m-%dT%H:%M:%fZ',updated_at)<>updated_at",
        );
        for column in table.columns.values() {
            predicate.push_str(" OR NOT (");
            predicate.push_str(&ddl::column_check(column));
            predicate.push(')');
            if column.required {
                predicate.push_str(" OR ");
                predicate.push_str(&quoted(&column.name));
                predicate.push_str(" IS NULL");
            }
        }
        let mut statement =
            connection.prepare(&format!("SELECT ({predicate}),* FROM {}", quoted(name)))?;
        let count = statement.column_count();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, bool>(0)? {
                return Err(MigrationError::InvalidData(format!(
                    "{name} 的存量行不满足冻结字段、系统 ID 或时间约束"
                )));
            }
            for index in 1..count {
                if let rusqlite::types::ValueRef::Text(bytes) = row.get_ref(index)?
                    && std::str::from_utf8(bytes).is_err()
                {
                    return Err(MigrationError::InvalidData(format!(
                        "{name} 含非法 UTF-8 文本，拒绝提交应用无法读取的数据"
                    )));
                }
            }
        }
    }
    Ok(())
}
