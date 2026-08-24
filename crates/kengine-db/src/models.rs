//! 业务模型动态建表与 CRUD：基于 `IR` 数据模型在 PostgreSQL 中管理业务数据。

use std::collections::HashMap;
use std::fmt::Write as _;

use kengine::ir::{FieldType, FilterOp, Model};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::{PgPool, Row};

use crate::DbError;

/// 业务表名：`kv_<模型名>`（中文模型名可直接作标识符，PG 支持 Unicode）。
#[must_use]
pub fn table_name(model: &str) -> String {
    format!("kv_{model}")
}

/// 确保模型表存在（按 `IR` 字段建列）。
///
/// 列类型映射：`number` → `DOUBLE PRECISION`；`boolean` → `BOOLEAN`；
/// 其余（含 `date` / `select` / `user` / `department`）→ `TEXT`（demo 简化，后续可按需精化）。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn ensure_model_table(pool: &PgPool, model: &Model) -> Result<(), DbError> {
    let table = table_name(&model.name);
    let mut cols: Vec<String> = vec!["id BIGSERIAL PRIMARY KEY".to_string()];
    for field in &model.fields {
        cols.push(format!("\"{}\" {}", field.name, sql_type(field.field_type)));
    }
    cols.push("created_at TIMESTAMPTZ NOT NULL DEFAULT now()".to_string());
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{table}\" ({})",
        cols.join(", ")
    );
    sqlx::query(&sql).execute(pool).await?;
    // 模型演进：为已有表补充缺失列（幂等）
    for field in &model.fields {
        let alter = format!(
            "ALTER TABLE \"{table}\" ADD COLUMN IF NOT EXISTS \"{}\" {}",
            field.name,
            sql_type(field.field_type)
        );
        sqlx::query(&alter).execute(pool).await?;
    }
    Ok(())
}

fn sql_type(ft: FieldType) -> &'static str {
    match ft {
        FieldType::Number => "DOUBLE PRECISION",
        FieldType::Boolean => "BOOLEAN",
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Date
        | FieldType::Select
        | FieldType::File
        | FieldType::User
        | FieldType::Department => "TEXT",
    }
}

/// 插入记录，返回新记录 id。
///
/// 仅插入 `data` 中出现的模型字段；缺省字段使用表默认值。
///
/// # Panics
/// 当 `data` 中出现模型定义之外的字段名时（内部调用保证不会发生）panic。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
#[allow(clippy::implicit_hasher)] // 内部 API，默认哈希器足够
pub async fn insert_record(
    pool: &PgPool,
    model: &Model,
    data: &HashMap<String, Value>,
) -> Result<i64, DbError> {
    let table = table_name(&model.name);
    let mut cols: Vec<String> = Vec::new();
    let mut placeholders: Vec<String> = Vec::new();

    let mut fields: Vec<(String, FieldType)> = Vec::new();
    for field in &model.fields {
        if data.contains_key(&field.name) {
            cols.push(format!("\"{}\"", field.name));
            placeholders.push(format!("${}", cols.len()));
            fields.push((field.name.clone(), field.field_type));
        }
    }
    if cols.is_empty() {
        let sql = format!("INSERT INTO \"{table}\" DEFAULT VALUES RETURNING id");
        let row = sqlx::query(&sql).fetch_one(pool).await?;
        return Ok(row.try_get("id")?);
    }

    let sql = format!(
        "INSERT INTO \"{table}\" ({}) VALUES ({}) RETURNING id",
        cols.join(", "),
        placeholders.join(", ")
    );
    let mut query = sqlx::query(&sql);
    for (name, ft) in &fields {
        let val = data.get(name).expect("字段存在性已检查");
        query = match ft {
            FieldType::Number => query.bind(val.as_f64().unwrap_or(0.0)),
            FieldType::Boolean => query.bind(val.as_bool().unwrap_or(false)),
            _ => match val {
                Value::Null => query.bind(Option::<String>::None),
                other => query.bind(other.as_str().map(str::to_string)),
            },
        };
    }
    let row = query.fetch_one(pool).await?;
    Ok(row.try_get("id")?)
}

/// 列出模型全部记录（含 `id` / `created_at`），按 `id` 升序。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn list_records(pool: &PgPool, model: &Model) -> Result<Vec<Value>, DbError> {
    list_records_where(pool, model, &[]).await
}

/// 过滤条件（行级 `DataFilter`）。
#[derive(Debug, Clone)]
pub struct FilterCond {
    /// 字段名。
    pub field: String,
    /// 比较操作符。
    pub op: FilterOp,
    /// 比较值。
    pub value: String,
}

impl FilterCond {
    /// 构造相等条件。
    #[must_use]
    pub fn eq(field: &str, value: &str) -> Self {
        Self {
            field: field.to_string(),
            op: FilterOp::Eq,
            value: value.to_string(),
        }
    }

    /// SQL 操作符。
    fn sql_op(&self) -> &'static str {
        match self.op {
            FilterOp::Eq => "=",
            FilterOp::Ne => "<>",
            FilterOp::Gt => ">",
            FilterOp::Ge => ">=",
            FilterOp::Lt => "<",
            FilterOp::Le => "<=",
        }
    }
}

/// 列出模型记录，支持行级过滤（`groups` 之间 OR 连接，组内 AND）。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn list_records_filtered(
    pool: &PgPool,
    model: &Model,
    groups: &[Vec<FilterCond>],
) -> Result<Vec<Value>, DbError> {
    let table = table_name(&model.name);
    let mut sql = format!("SELECT to_jsonb(t) FROM \"{table}\" t");
    let mut n = 0;
    if !groups.is_empty() {
        let mut or_clauses = Vec::with_capacity(groups.len());
        for group in groups {
            let mut and_clauses = Vec::with_capacity(group.len());
            for c in group {
                n += 1;
                and_clauses.push(format!("t.\"{}\" {} ${n}", c.field, c.sql_op()));
            }
            or_clauses.push(format!("({})", and_clauses.join(" AND ")));
        }
        let _ = write!(sql, " WHERE {}", or_clauses.join(" OR "));
    }
    sql.push_str(" ORDER BY t.id");
    let mut query = sqlx::query_as::<_, (Json<Value>,)>(&sql);
    for group in groups {
        for c in group {
            query = query.bind(&c.value);
        }
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(json,)| json.0).collect())
}

/// 列出模型记录，支持 AND 相等过滤（旧接口兼容）。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn list_records_where(
    pool: &PgPool,
    model: &Model,
    conditions: &[(String, String)],
) -> Result<Vec<Value>, DbError> {
    let groups: Vec<Vec<FilterCond>> = if conditions.is_empty() {
        Vec::new()
    } else {
        vec![
            conditions
                .iter()
                .map(|(f, v)| FilterCond::eq(f, v))
                .collect(),
        ]
    };
    list_records_filtered(pool, model, &groups).await
}

/// 模型存储操作集合。
pub struct ModelStore<'a> {
    pool: &'a PgPool,
}

impl<'a> ModelStore<'a> {
    /// 创建模型存储。
    #[must_use]
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// 确保模型表存在。
    ///
    /// # Errors
    /// 见 [`ensure_model_table`]。
    pub async fn ensure(&self, model: &Model) -> Result<(), DbError> {
        ensure_model_table(self.pool, model).await
    }

    /// 插入记录。
    ///
    /// # Errors
    /// 见 [`insert_record`]。
    pub async fn insert(
        &self,
        model: &Model,
        data: &HashMap<String, Value>,
    ) -> Result<i64, DbError> {
        insert_record(self.pool, model, data).await
    }

    /// 列出记录。
    ///
    /// # Errors
    /// 见 [`list_records`]。
    pub async fn list(&self, model: &Model) -> Result<Vec<Value>, DbError> {
        list_records(self.pool, model).await
    }

    /// 按过滤条件列出记录（行级过滤）。
    ///
    /// # Errors
    /// 见 [`list_records_where`]。
    pub async fn list_where(
        &self,
        model: &Model,
        conditions: &[(String, String)],
    ) -> Result<Vec<Value>, DbError> {
        list_records_where(self.pool, model, conditions).await
    }

    /// 按 OR 分组条件列出记录（行级过滤：组内 AND、组间 OR，支持比较操作符）。
    ///
    /// # Errors
    /// 见 [`list_records_filtered`]。
    pub async fn list_filtered(
        &self,
        model: &Model,
        groups: &[Vec<FilterCond>],
    ) -> Result<Vec<Value>, DbError> {
        list_records_filtered(self.pool, model, groups).await
    }
}
