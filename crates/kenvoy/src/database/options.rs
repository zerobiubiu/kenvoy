//! 仅投影已注册选项列；客户端不能传入资源名、列名或任意 SQL 条件。

use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    DatabaseError, Result,
    query::{identifier, sql_value},
};
use crate::{
    application::{ApplicationIR, FieldDefinition, ResourceDefinition},
    options, validation,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OptionsQuery {
    pub dependencies: Map<String, Value>,
    pub search: Option<String>,
    pub page: u32,
    pub page_size: u32,
    pub selected: Option<String>,
}
impl Default for OptionsQuery {
    fn default() -> Self {
        Self {
            dependencies: Map::new(),
            search: None,
            page: 1,
            page_size: 20,
            selected: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionItem {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionsResult {
    pub data: Vec<OptionItem>,
    pub selected: Option<OptionItem>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

fn predicate(
    field: &FieldDefinition,
    application: &ApplicationIR,
    record: &Map<String, Value>,
) -> Result<Option<(String, Vec<SqlValue>)>> {
    let source = field
        .options_source
        .as_ref()
        .ok_or_else(|| DatabaseError::InvalidQuery("字段没有远程选项来源".into()))?;
    let target = application
        .resource(&source.resource)
        .ok_or_else(|| DatabaseError::UnknownResource(source.resource.clone()))?;
    let value_column = if source.value == "id" {
        "CAST(\"id\" AS TEXT)".to_owned()
    } else {
        identifier(&source.value)
    };
    let mut clauses = vec![
        format!("{value_column} IS NOT NULL"),
        format!("length({value_column})>0"),
        format!("{} IS NOT NULL", identifier(&source.label)),
    ];
    let mut values = Vec::new();
    for (target_name, local_name) in &source.filters {
        let Some(value) = record.get(local_name).filter(|value| !value.is_null()) else {
            return Ok(None);
        };
        let target_field = target
            .field(target_name)
            .ok_or_else(|| DatabaseError::InvalidState("选项筛选字段未注册".into()))?;
        let value = match validation::validate_field(target_field, value) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        clauses.push(format!("{} = ?", identifier(target_name)));
        values.push(sql_value(&value)?);
    }
    Ok(Some((clauses.join(" AND "), values)))
}

fn value_column(field: &FieldDefinition) -> Result<String> {
    let source = field
        .options_source
        .as_ref()
        .ok_or_else(|| DatabaseError::InvalidQuery("字段没有远程选项来源".into()))?;
    Ok(if source.value == "id" {
        "CAST(\"id\" AS TEXT)".into()
    } else {
        identifier(&source.value)
    })
}

pub(super) fn validate_record(
    connection: &Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    values: &Map<String, Value>,
) -> Result<()> {
    options::validate_static(resource, values)?;
    for field in &resource.fields {
        let Some(source) = &field.options_source else {
            continue;
        };
        let Some(value) = values.get(&field.name).filter(|value| !value.is_null()) else {
            continue;
        };
        let Some((predicate, mut params)) = predicate(field, application, values)? else {
            return Err(options::invalid(field).into());
        };
        params.push(sql_value(value)?);
        let exists: bool = connection.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM {} WHERE {predicate} AND {} = ?)",
                identifier(&source.resource),
                value_column(field)?
            ),
            params_from_iter(params),
            |row| row.get(0),
        )?;
        if !exists {
            return Err(options::invalid(field).into());
        }
    }
    Ok(())
}

pub(super) fn query(
    connection: &mut Connection,
    application: &ApplicationIR,
    resource: &ResourceDefinition,
    field_name: &str,
    query: OptionsQuery,
) -> Result<OptionsResult> {
    let field = resource
        .field(field_name)
        .ok_or_else(|| DatabaseError::InvalidQuery("选项字段未注册".into()))?;
    let source = field
        .options_source
        .as_ref()
        .ok_or_else(|| DatabaseError::InvalidQuery("字段没有远程选项来源".into()))?;
    if query.page == 0 || !(1..=100).contains(&query.page_size) {
        return Err(DatabaseError::InvalidQuery(
            "page 从 1 开始，page_size 为 1..100".into(),
        ));
    }
    if query
        .search
        .as_ref()
        .is_some_and(|text| text.chars().count() > 256 || text.contains('\0'))
        || query
            .selected
            .as_ref()
            .is_some_and(|text| text.chars().count() > 1_000_000 || text.contains('\0'))
    {
        return Err(DatabaseError::InvalidQuery(
            "选项查询文本超过限制或包含 NUL".into(),
        ));
    }
    let dependencies = field.option_dependencies();
    if query
        .dependencies
        .keys()
        .any(|name| !dependencies.contains(name.as_str()))
    {
        return Err(DatabaseError::InvalidQuery(
            "只能提交该字段声明的联动依赖".into(),
        ));
    }
    for (name, value) in &query.dependencies {
        let definition = resource
            .field(name)
            .ok_or_else(|| DatabaseError::InvalidQuery("联动字段未注册".into()))?;
        if !value.is_null() {
            validation::validate_field(definition, value).map_err(|error| {
                DatabaseError::Validation(validation::ValidationErrors(vec![error]))
            })?;
        }
    }
    let mut result = OptionsResult {
        data: vec![],
        selected: None,
        total: 0,
        page: query.page,
        page_size: query.page_size,
    };
    let Some((base_predicate, base_params)) = predicate(field, application, &query.dependencies)?
    else {
        return Ok(result);
    };
    let value = value_column(field)?;
    let label = identifier(&source.label);
    let table = identifier(&source.resource);
    let transaction = connection.transaction()?;
    if let Some(selected) = query.selected {
        let mut params = base_params.clone();
        params.push(SqlValue::Text(selected));
        let mut statement = transaction.prepare(&format!(
            "SELECT {value},{label} FROM {table} WHERE {base_predicate} AND {value} = ? LIMIT 1"
        ))?;
        result.selected = statement
            .query(params_from_iter(params))?
            .next()?
            .map(|row| {
                Ok::<_, rusqlite::Error>(OptionItem {
                    value: row.get(0)?,
                    label: row.get(1)?,
                })
            })
            .transpose()?;
    }
    let mut params = base_params;
    let mut predicate = base_predicate;
    if let Some(search) = query.search.filter(|search| !search.is_empty()) {
        predicate.push_str(&format!(" AND {label} LIKE ? ESCAPE '\\'"));
        params.push(SqlValue::Text(format!(
            "%{}%",
            search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )));
    }
    let count: i64 = transaction.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {predicate}"),
        params_from_iter(params.iter()),
        |row| row.get(0),
    )?;
    result.total =
        u64::try_from(count).map_err(|_| DatabaseError::CorruptData("选项总数为负".into()))?;
    params.push(SqlValue::Integer(i64::from(query.page_size)));
    params.push(SqlValue::Integer(
        i64::from(query.page - 1) * i64::from(query.page_size),
    ));
    {
        let mut statement = transaction.prepare(&format!("SELECT {value},{label} FROM {table} WHERE {predicate} ORDER BY {label},id LIMIT ? OFFSET ?"))?;
        let mut rows = statement.query(params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            result.data.push(OptionItem {
                value: row.get(0)?,
                label: row.get(1)?,
            });
        }
    }
    transaction.commit()?;
    Ok(result)
}
