//! 业务模型动态建表与 CRUD：基于 `IR` 数据模型在 PostgreSQL 中管理业务数据。

use std::collections::HashMap;
use std::fmt::Write as _;

use kengine::ir::{FieldType, FilterOp, Model};
use serde_json::{Value, json};
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
    cols.push("updated_at TIMESTAMPTZ NOT NULL DEFAULT now()".to_string());
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
    // 系统列演进（旧表缺列时补充）
    for (col, def) in [
        ("created_at", "TIMESTAMPTZ NOT NULL DEFAULT now()"),
        ("updated_at", "TIMESTAMPTZ NOT NULL DEFAULT now()"),
    ] {
        let alter = format!("ALTER TABLE \"{table}\" ADD COLUMN IF NOT EXISTS \"{col}\" {def}");
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

/// 更新记录的部分字段（`data` 中出现且属于模型定义的字段）。
///
/// 返回 `false` 表示记录不存在。
///
/// # Panics
/// 当 `data` 中出现模型定义之外的字段名时（内部调用保证不会发生）panic。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
#[allow(clippy::implicit_hasher)] // 内部 API，默认哈希器足够
pub async fn update_record(
    pool: &PgPool,
    model: &Model,
    id: i64,
    data: &HashMap<String, Value>,
) -> Result<bool, DbError> {
    let table = table_name(&model.name);
    let mut sets: Vec<String> = Vec::new();
    let mut fields: Vec<(String, FieldType)> = Vec::new();
    for field in &model.fields {
        if data.contains_key(&field.name) {
            sets.push(format!("\"{}\" = ${}", field.name, sets.len() + 1));
            fields.push((field.name.clone(), field.field_type));
        }
    }
    if sets.is_empty() {
        return Ok(true); // 无可更新字段，幂等成功
    }
    sets.push("updated_at = now()".to_string());
    let sql = format!(
        "UPDATE \"{table}\" SET {} WHERE id = ${}",
        sets.join(", "),
        fields.len() + 1
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
    let row = query.bind(id).execute(pool).await?;
    Ok(row.rows_affected() > 0)
}

/// 删除记录，返回 `false` 表示记录不存在。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn delete_record(pool: &PgPool, model: &Model, id: i64) -> Result<bool, DbError> {
    let table = table_name(&model.name);
    let row = sqlx::query(&format!("DELETE FROM \"{table}\" WHERE id = $1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(row.rows_affected() > 0)
}

/// 分组统计：`SELECT 字段, count|sum|avg(字段) FROM 模型 GROUP BY 字段`。
///
/// 返回 `[{ "group": 分组值, "value": 统计值 }]`（按统计值降序）。
/// `stat` 仅支持 `count` / `sum` / `avg`。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn stats_group(
    pool: &PgPool,
    model: &Model,
    field: &str,
    stat: &str,
) -> Result<Vec<Value>, DbError> {
    let table = table_name(&model.name);
    let agg = match stat {
        "sum" => "sum",
        "avg" => "avg",
        _ => "count",
    };
    let agg_sql = if agg == "count" {
        "CAST(count(*) AS DOUBLE PRECISION)".to_string()
    } else {
        format!("CAST({agg}(\"{field}\") AS DOUBLE PRECISION)")
    };
    let sql = format!(
        "SELECT \"{field}\" AS grp, {agg_sql} AS val FROM \"{table}\" \
         WHERE \"{field}\" IS NOT NULL GROUP BY \"{field}\" ORDER BY val DESC"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let group: String = row.try_get("grp")?;
        let value: f64 = row.try_get("val")?;
        out.push(json!({ "group": group, "value": value }));
    }
    Ok(out)
}
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

/// 列出模型记录，支持行级过滤（`groups` 之间 OR 连接，组内 AND）与排序、分页。
///
/// `order_by` 为 `(字段, asc|desc)` 列表（字段必须为模型字段，否则忽略该条）。
///
/// # Errors
/// SQL 执行失败时返回 [`DbError`]。
pub async fn list_records_query(
    pool: &PgPool,
    model: &Model,
    groups: &[Vec<FilterCond>],
    order_by: &[(String, String)],
    limit: Option<i64>,
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
                // 值按字段类型显式 cast（`id` 为 BIGSERIAL；数字/布尔字段同理），
                // 避免 TEXT 参数与数值列比较类型不匹配
                let cast = if c.field == "id" {
                    "::BIGINT"
                } else {
                    match model
                        .fields
                        .iter()
                        .find(|f| f.name == c.field)
                        .map(|f| f.field_type)
                    {
                        Some(FieldType::Number) => "::DOUBLE PRECISION",
                        Some(FieldType::Boolean) => "::BOOLEAN",
                        _ => "",
                    }
                };
                and_clauses.push(format!("t.\"{}\" {} ${n}{cast}", c.field, c.sql_op()));
            }
            or_clauses.push(format!("({})", and_clauses.join(" AND ")));
        }
        let _ = write!(sql, " WHERE {}", or_clauses.join(" OR "));
    }
    if !order_by.is_empty() {
        let mut orders = Vec::with_capacity(order_by.len());
        for (field, dir) in order_by {
            let dir = if dir == "desc" { "DESC" } else { "ASC" };
            if model.fields.iter().any(|f| &f.name == field) {
                orders.push(format!("t.\"{field}\" {dir}"));
            }
        }
        if !orders.is_empty() {
            let _ = write!(sql, " ORDER BY {}", orders.join(", "));
        }
    }
    if !sql.contains(" ORDER BY ") {
        sql.push_str(" ORDER BY t.id");
    }
    // `LIMIT` 占位符接在全部条件占位符之后，避免与 `$1` 冲突
    let _ = write!(sql, " LIMIT ${}", n + 1);
    let mut query = sqlx::query_as::<_, (Json<Value>,)>(&sql);
    // bind 顺序与占位符编号一致：先条件值（$1..$n），再 limit（${n+1}）
    for group in groups {
        for c in group {
            query = query.bind(&c.value);
        }
    }
    query = query.bind(limit.unwrap_or(500).clamp(1, 5000));
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(json,)| json.0).collect())
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
    list_records_query(pool, model, groups, &[], None).await
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

    /// 按条件 + 排序 + 分页列出记录。
    ///
    /// # Errors
    /// 见 [`list_records_query`]。
    pub async fn list_query(
        &self,
        model: &Model,
        groups: &[Vec<FilterCond>],
        order_by: &[(String, String)],
        limit: Option<i64>,
    ) -> Result<Vec<Value>, DbError> {
        list_records_query(self.pool, model, groups, order_by, limit).await
    }

    /// 更新记录部分字段。
    ///
    /// # Errors
    /// 见 [`update_record`]。
    pub async fn update(
        &self,
        model: &Model,
        id: i64,
        data: &HashMap<String, Value>,
    ) -> Result<bool, DbError> {
        update_record(self.pool, model, id, data).await
    }

    /// 删除记录。
    ///
    /// # Errors
    /// 见 [`delete_record`]。
    pub async fn delete(&self, model: &Model, id: i64) -> Result<bool, DbError> {
        delete_record(self.pool, model, id).await
    }
}
