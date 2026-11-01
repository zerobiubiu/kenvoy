//! 类型化筛选、稳定排序与单事务分页读取。

use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde_json::Value;

use crate::{
    application::resources::{FilterOperator, ListQuery, ListResult},
    domain::{
        model::{FieldDefinition, FieldKind, ResourceDefinition},
        validation,
    },
};

use super::super::{DatabaseError, Result};
use super::{columns, decode, identifier, sql_value};

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
    if field.kind == FieldKind::File {
        return Err(DatabaseError::InvalidQuery("附件字段不支持标量筛选".into()));
    }
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

pub(in super::super) fn list(
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
    if resource
        .field(sort)
        .is_some_and(|field| field.kind == FieldKind::File)
    {
        return Err(DatabaseError::InvalidQuery("附件字段不支持标量排序".into()));
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
