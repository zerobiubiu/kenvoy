//! 受检 Schema 模型及稳定序列化、哈希与标识符规则。

mod constraints;
pub(super) mod ddl;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::migration::{MigrationError, Result};
use crate::domain::{
    model::{ApplicationIR, FieldKind, FileConstraints},
    validation::validate_default,
};
use constraints::validate_constraints;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SchemaModel {
    pub tables: BTreeMap<String, TableModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableModel {
    pub columns: BTreeMap<String, ColumnModel>,
    pub indexes: BTreeMap<String, IndexModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnModel {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
    pub integer: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub max_length: Option<usize>,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<FileConstraints>,
}

fn deserialize_default<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexModel {
    pub column: String,
    pub unique: bool,
}

/// 重新验证从产物加载的业务标识和约束，避免将伪造 IR 的名称插入 SQL。
pub fn schema_model(application: &ApplicationIR) -> Result<SchemaModel> {
    crate::domain::options::validate_definitions(&application.resources)
        .map_err(|(_, message)| MigrationError::InvalidSchema(message))?;
    let mut tables = BTreeMap::new();
    let mut index_names = BTreeSet::new();
    for resource in &application.resources {
        validate_business_identifier(&resource.name)?;
        if resource.fields.is_empty() {
            return Err(MigrationError::InvalidSchema(format!(
                "资源 {} 至少需要一个字段",
                resource.name
            )));
        }
        let mut columns = BTreeMap::new();
        let mut indexes = BTreeMap::new();
        for field in &resource.fields {
            validate_business_identifier(&field.name)?;
            validate_constraints(field)?;
            let mut options: Vec<_> = field
                .options
                .iter()
                .map(|option| option.value.clone())
                .collect();
            options.sort();
            let default = field
                .default
                .as_ref()
                .map(|value| {
                    validate_default(field, value)
                        .map_err(|error| MigrationError::InvalidSchema(error.to_string()))
                })
                .transpose()?;
            let column = ColumnModel {
                name: field.name.clone(),
                kind: if field.kind == FieldKind::Textarea {
                    FieldKind::Text
                } else {
                    field.kind
                },
                required: field.required,
                default,
                integer: field.integer,
                min: field
                    .min
                    .map(|value| if value == 0.0 { 0.0 } else { value }),
                max: field
                    .max
                    .map(|value| if value == 0.0 { 0.0 } else { value }),
                max_length: field.max_length,
                options,
                upload: field.upload.clone(),
            };
            if columns.insert(field.name.clone(), column).is_some() {
                return Err(MigrationError::InvalidSchema(format!(
                    "资源 {} 包含重复字段 {}",
                    resource.name, field.name
                )));
            }
            if field.unique || field.index {
                let name = format!(
                    "{}_{}_{}",
                    if field.unique { "uq" } else { "idx" },
                    resource.name,
                    field.name
                );
                if !index_names.insert(name.clone()) {
                    return Err(MigrationError::InvalidSchema(format!(
                        "生成的索引名称 {name} 冲突"
                    )));
                }
                indexes.insert(
                    name,
                    IndexModel {
                        column: field.name.clone(),
                        unique: field.unique,
                    },
                );
            }
        }
        if tables
            .insert(resource.name.clone(), TableModel { columns, indexes })
            .is_some()
        {
            return Err(MigrationError::InvalidSchema(format!(
                "重复资源 {}",
                resource.name
            )));
        }
    }
    Ok(SchemaModel { tables })
}

pub fn validate_identifier(name: &str) -> Result<()> {
    let mut chars = name.chars();
    if !chars.next().is_some_and(|first| first.is_ascii_lowercase())
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        || name.starts_with("sqlite_")
    {
        return Err(MigrationError::InvalidSchema(format!(
            "非法数据库标识符 {name}"
        )));
    }
    Ok(())
}

fn validate_business_identifier(name: &str) -> Result<()> {
    validate_identifier(name)?;
    if name.starts_with("kenvoy_")
        || matches!(
            name,
            "id" | "created_at" | "updated_at" | "actions" | "auth"
        )
    {
        return Err(MigrationError::InvalidSchema(format!(
            "{name} 是框架保留业务名称"
        )));
    }
    Ok(())
}

/// 对外只接受已验证资源/字段名，系统表由本模块的固定 SQL 管理。
pub fn quote_identifier(name: &str) -> Result<String> {
    validate_identifier(name)?;
    Ok(quoted(name))
}
pub(super) fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
pub fn schema_hash(model: &SchemaModel) -> Result<String> {
    hash(model)
}
pub(super) fn hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(Sha256::digest(serde_json::to_vec(value)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
