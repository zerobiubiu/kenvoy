//! sqlite-schema-v1 的冻结结构核验；不读取 ApplicationIR 或调用业务校验回调。
use std::collections::BTreeSet;

use crate::domain::{model::FieldKind, validation::validate_file_constraints};
use rusqlite::Connection;

use super::super::migration::{MigrationError, Result};
use super::{SchemaModel, ddl, validate_business_identifier, validate_identifier};

pub(in super::super) fn validate(model: &SchemaModel) -> Result<()> {
    let mut indexes = BTreeSet::new();
    for (name, table) in &model.tables {
        validate_business_identifier(name)?;
        if table.columns.is_empty() {
            return Err(invalid("冻结表不得没有业务字段"));
        }
        for (name, column) in &table.columns {
            validate_business_identifier(name)?;
            if &column.name != name {
                return Err(invalid("冻结列名与键不一致"));
            }
            match (column.kind, &column.upload) {
                (FieldKind::File, Some(upload)) => {
                    validate_file_constraints(upload).map_err(invalid)?
                }
                (FieldKind::File, None) | (_, Some(_)) => {
                    return Err(invalid("冻结附件字段与 upload 参数不一致"));
                }
                _ => {}
            }
            if column.min.is_some_and(|v| !v.is_finite())
                || column.max.is_some_and(|v| !v.is_finite())
                || matches!((column.min, column.max), (Some(a), Some(b)) if a > b)
                || column.options.windows(2).any(|v| v[0] >= v[1])
            {
                return Err(invalid("冻结字段范围或选项不规范"));
            }
        }
        for (name, index) in &table.indexes {
            validate_identifier(name)?;
            if name.starts_with("kenvoy_")
                || !indexes.insert(name)
                || !table.columns.contains_key(&index.column)
            {
                return Err(invalid("冻结索引重复、保留或引用不存在的列"));
            }
        }
    }
    let scratch = Connection::open_in_memory()?;
    create(&scratch, model)
}

pub(in super::super) fn create(connection: &Connection, model: &SchemaModel) -> Result<()> {
    for (name, table) in &model.tables {
        connection.execute_batch(&ddl::create_table(name, table))?;
        ddl::create_indexes(connection, name, table)?;
    }
    Ok(())
}

/// 比较由 SQLite 解析过的 DDL；列顺序/空白/标识符引号不影响等价性，约束内容必须完全一致。
pub(in super::super) fn verify(connection: &Connection, model: &SchemaModel) -> Result<()> {
    let expected = Connection::open_in_memory()?;
    create(&expected, model)?;
    if fingerprint(connection)? != fingerprint(&expected)? {
        return Err(MigrationError::Drift(
            "实际业务结构与冻结目标 Schema 不一致".into(),
        ));
    }
    super::frozen_data::verify(connection, model)
}

type ObjectSignature = (String, String, Vec<Vec<String>>);

fn fingerprint(connection: &Connection) -> Result<Vec<ObjectSignature>> {
    let objects = super::super::migration::state::actual_objects(connection)?;
    let mut output = vec![];
    for object in objects {
        let sql = object
            .sql
            .as_deref()
            .ok_or_else(|| invalid("业务对象缺少固定 DDL"))?;
        let mut tokens = tokenize(sql)?;
        if object.kind == "table" {
            let start = tokens
                .iter()
                .position(|t| t == "(")
                .ok_or_else(|| invalid("表 DDL 缺少列定义"))?;
            if tokens.last().is_none_or(|t| t != ")") {
                return Err(invalid("表 DDL 含不支持的尾部定义"));
            }
            tokens = tokens[start + 1..tokens.len() - 1].to_vec();
            let mut depth = 0_i64;
            let mut definitions = vec![vec![]];
            for token in tokens {
                match token.as_str() {
                    "(" => depth += 1,
                    ")" => depth -= 1,
                    "," if depth == 0 => {
                        definitions.push(vec![]);
                        continue;
                    }
                    _ => {}
                }
                definitions.last_mut().expect("定义列表不为空").push(token);
            }
            definitions.sort();
            output.push((object.kind, object.name, definitions));
        } else {
            output.push((object.kind, object.name, vec![tokens]));
        }
    }
    Ok(output)
}

// 此词法器只用于比较 SQLite 已成功解析的 DDL，不负责拆分或执行用户 SQL。
fn tokenize(sql: &str) -> Result<Vec<String>> {
    let chars: Vec<_> = sql.chars().collect();
    let mut at = 0;
    let mut out = vec![];
    while at < chars.len() {
        let ch = chars[at];
        if ch.is_whitespace() || ch == ';' {
            at += 1;
            continue;
        }
        if matches!(ch, '\'' | '"' | '`' | '[') {
            let end = if ch == '[' { ']' } else { ch };
            at += 1;
            let mut value = String::new();
            loop {
                if at == chars.len() {
                    return Err(invalid("DDL 引号未闭合"));
                }
                if chars[at] == end {
                    at += 1;
                    if at < chars.len() && chars[at] == end && end != ']' {
                        value.push(end);
                        at += 1;
                        continue;
                    }
                    break;
                }
                value.push(chars[at]);
                at += 1;
            }
            out.push(if ch == '\'' {
                format!("'{value}'")
            } else {
                value.to_ascii_lowercase()
            });
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            let start = at;
            while at < chars.len()
                && (chars[at].is_ascii_alphanumeric() || matches!(chars[at], '_' | '.'))
            {
                at += 1;
            }
            out.push(
                chars[start..at]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
        } else {
            out.push(ch.to_string());
            at += 1;
        }
    }
    Ok(out)
}
fn invalid(message: &str) -> MigrationError {
    MigrationError::InvalidSchema(message.into())
}
