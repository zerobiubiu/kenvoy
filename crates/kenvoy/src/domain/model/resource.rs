use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::source::SourceLocation;

use super::permission::Role;

fn deserialize_default<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceDefinition {
    pub name: String,
    pub fields: Vec<FieldDefinition>,
    pub source: SourceLocation,
}

impl ResourceDefinition {
    pub fn field(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|field| field.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Textarea,
    Number,
    Boolean,
    Date,
    Datetime,
    Select,
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    /// None 为未声明默认值，Some(Null) 为显式默认 null。
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
    pub unique: bool,
    pub index: bool,
    pub options: Vec<SelectOption>,
    #[serde(default)]
    pub options_source: Option<OptionsSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<FileConstraints>,
    pub source: SourceLocation,
}

impl FieldDefinition {
    /// 页面 label 和声明来源不影响持久化 Schema 兼容性。
    pub fn same_schema(&self, other: &Self) -> bool {
        self.name == other.name
            && self.kind == other.kind
            && self.required == other.required
            && self.default == other.default
            && self.integer == other.integer
            && self.min == other.min
            && self.max == other.max
            && self.max_length == other.max_length
            && self.unique == other.unique
            && self.index == other.index
            && self.options_source == other.options_source
            && self.upload == other.upload
            && self
                .options
                .iter()
                .map(|option| (&option.value, &option.when))
                .eq(other
                    .options
                    .iter()
                    .map(|option| (&option.value, &option.when)))
    }
}

pub const MAX_UPLOAD_SIZE: u64 = 10 * 1024 * 1024;
pub const MAX_UPLOAD_FILES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConstraints {
    pub accept: Vec<String>,
    pub max_size: u64,
    pub max_files: usize,
}

impl Default for FileConstraints {
    fn default() -> Self {
        Self {
            accept: Vec::new(),
            max_size: MAX_UPLOAD_SIZE,
            max_files: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadedFile {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub when: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsSource {
    pub resource: String,
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub filters: BTreeMap<String, String>,
    pub permission: Role,
}

impl FieldDefinition {
    pub fn option_dependencies(&self) -> std::collections::BTreeSet<&str> {
        self.options
            .iter()
            .flat_map(|option| option.when.keys().map(String::as_str))
            .chain(
                self.options_source
                    .iter()
                    .flat_map(|source| source.filters.values().map(String::as_str)),
            )
            .collect()
    }
}
