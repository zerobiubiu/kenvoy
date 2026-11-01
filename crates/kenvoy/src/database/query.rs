use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{
    Connection, Row, TransactionBehavior, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::{
    application::{ApplicationIR, FieldDefinition, FieldKind, ResourceDefinition},
    validation::{self, WriteMode},
};

use super::{DatabaseError, Result};

pub type Record = Map<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldFilter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListQuery {
    pub page: u32,
    pub page_size: u32,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub descending: bool,
    pub filters: Vec<FieldFilter>,
}
impl Default for ListQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 20,
            search: None,
            sort: None,
            descending: false,
            filters: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListResult {
    pub items: Vec<Record>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

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
        _ => Err(DatabaseError::InvalidState(
            "资源字段只支持 JSON 标量".into(),
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

pub(super) fn create(
    connection: &mut Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    input: &Value,
) -> Result<Record> {
    let values = validation::validate_record(resource, input, WriteMode::Create)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    super::options::validate_record(&transaction, application, resource, &values)?;
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
    let result = get(&transaction, resource, id)?
        .ok_or_else(|| DatabaseError::InvalidState("写入后未找到记录".into()))?;
    transaction.commit()?;
    Ok(result)
}

pub(super) fn update(
    connection: &mut Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    id: i64,
    input: &Value,
) -> Result<Option<Record>> {
    valid_id(id)?;
    let values = validation::validate_record(resource, input, WriteMode::Patch)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(existing) = get(&transaction, resource, id)? else {
        return Ok(None);
    };
    let mut merged = existing.clone();
    merged.extend(values.clone());
    super::options::validate_record(&transaction, application, resource, &merged)?;
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
    let result = get(&transaction, resource, id)?;
    transaction.commit()?;
    Ok(result)
}

pub(super) fn delete(
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
    transaction.commit()?;
    Ok(changed == 1)
}

fn like_literal(value: &str) -> String {
    format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

fn filter(
    field: &FieldDefinition,
    operator: FilterOperator,
    value: &Value,
) -> Result<(String, Option<SqlValue>)> {
    let name = identifier(&field.name);
    if value.is_null() {
        return match operator {
            FilterOperator::Eq => Ok((format!("{name} IS NULL"), None)),
            FilterOperator::NotEq => Ok((format!("{name} IS NOT NULL"), None)),
            _ => Err(DatabaseError::InvalidQuery(
                "null 筛选只支持 eq/not_eq".into(),
            )),
        };
    }
    if operator == FilterOperator::Contains {
        if !matches!(
            field.kind,
            FieldKind::Text | FieldKind::Textarea | FieldKind::Select
        ) {
            return Err(DatabaseError::InvalidQuery(
                "contains 只支持文本字段".into(),
            ));
        }
        let text = value
            .as_str()
            .filter(|text| text.chars().count() <= 256 && !text.contains('\0'))
            .ok_or_else(|| {
                DatabaseError::InvalidQuery("contains 必须是不含 NUL、最多 256 字符的字符串".into())
            })?;
        return Ok((
            format!("{name} LIKE ? ESCAPE '\\'"),
            Some(SqlValue::Text(like_literal(text))),
        ));
    }
    let normalized = validation::validate_field(field, value)
        .map_err(|error| DatabaseError::Validation(validation::ValidationErrors(vec![error])))?;
    let symbol = match operator {
        FilterOperator::Eq => "=",
        FilterOperator::NotEq => "!=",
        FilterOperator::Lt => "<",
        FilterOperator::Lte => "<=",
        FilterOperator::Gt => ">",
        FilterOperator::Gte => ">=",
        FilterOperator::Contains => unreachable!("contains 已单独处理"),
    };
    Ok((format!("{name} {symbol} ?"), Some(sql_value(&normalized)?)))
}

pub(super) fn list(
    connection: &mut Connection,
    resource: &ResourceDefinition,
    query: &ListQuery,
) -> Result<ListResult> {
    if query.page == 0 || !(1..=100).contains(&query.page_size) {
        return Err(DatabaseError::InvalidQuery(
            "page 从 1 开始，page_size 必须为 1..100".into(),
        ));
    }
    if query.filters.len() > 16 {
        return Err(DatabaseError::InvalidQuery("最多允许 16 个筛选条件".into()));
    }
    let sort = query.sort.as_deref().unwrap_or("id");
    if !matches!(sort, "id" | "created_at" | "updated_at") && resource.field(sort).is_none() {
        return Err(DatabaseError::InvalidQuery("排序字段未在资源中声明".into()));
    }
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    if let Some(search) = &query.search {
        if search.chars().count() > 256 || search.contains('\0') {
            return Err(DatabaseError::InvalidQuery(
                "搜索文本不能含 NUL，且最多 256 字符".into(),
            ));
        }
        if !search.is_empty() {
            let mut options = Vec::new();
            for field in &resource.fields {
                if matches!(
                    field.kind,
                    FieldKind::Text | FieldKind::Textarea | FieldKind::Select
                ) {
                    options.push(format!("{} LIKE ? ESCAPE '\\'", identifier(&field.name)));
                    params.push(SqlValue::Text(like_literal(search)));
                }
            }
            clauses.push(if options.is_empty() {
                "0".into()
            } else {
                format!("({})", options.join(" OR "))
            });
        }
    }
    for condition in &query.filters {
        let field = resource
            .field(&condition.field)
            .ok_or_else(|| DatabaseError::InvalidQuery("筛选字段未在资源中声明".into()))?;
        let (clause, value) = filter(field, condition.operator, &condition.value)?;
        clauses.push(clause);
        if let Some(value) = value {
            params.push(value);
        }
    }
    let predicate = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let direction = if query.descending { "DESC" } else { "ASC" };
    let ordering = if sort == "id" {
        format!("id {direction}")
    } else {
        format!("{} {direction}, id {direction}", identifier(sort))
    };
    let transaction = connection.transaction()?;
    let total: i64 = transaction.query_row(
        &format!(
            "SELECT COUNT(*) FROM {}{predicate}",
            identifier(&resource.name)
        ),
        params_from_iter(params.iter()),
        |row| row.get(0),
    )?;
    let total = u64::try_from(total)
        .map_err(|_| DatabaseError::CorruptData("记录计数不能为负数".into()))?;
    params.push(SqlValue::Integer(i64::from(query.page_size)));
    params.push(SqlValue::Integer(
        i64::from(query.page - 1) * i64::from(query.page_size),
    ));
    let mut items = Vec::new();
    {
        let mut statement = transaction.prepare(&format!(
            "SELECT {} FROM {}{predicate} ORDER BY {ordering} LIMIT ? OFFSET ?",
            columns(resource),
            identifier(&resource.name)
        ))?;
        let mut rows = statement.query(params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            items.push(decode(row, resource)?);
        }
    }
    transaction.commit()?;
    Ok(ListResult {
        items,
        total,
        page: query.page,
        page_size: query.page_size,
    })
}
