//! 经语义校验的宿主业务模型。数据库、HTTP 和客户端投影消费同一资源定义。

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dsl::SourceLocation;

pub const APPLICATION_IR_VERSION: u32 = 1;

fn deserialize_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != APPLICATION_IR_VERSION {
        return Err(serde::de::Error::custom(format!(
            "不支持 Application IR 版本 {version}"
        )));
    }
    Ok(version)
}

fn deserialize_default<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationIR {
    #[serde(deserialize_with = "deserialize_version")]
    pub format_version: u32,
    pub name: String,
    pub resources: Vec<ResourceDefinition>,
    pub pages: Vec<PageDefinition>,
    pub actions: Vec<ActionDefinition>,
    pub navigation: Vec<NavigationDefinition>,
    pub permissions: Vec<PermissionDefinition>,
    pub module_references: Vec<ModuleReference>,
    #[serde(default)]
    pub component_overrides: BTreeMap<String, String>,
}

impl ApplicationIR {
    pub fn resource(&self, name: &str) -> Option<&ResourceDefinition> {
        self.resources.iter().find(|resource| resource.name == name)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Anonymous,
    User,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageDefinition {
    pub path: String,
    pub title: String,
    pub permission: Role,
    pub components: Vec<ComponentDefinition>,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormMode {
    Create,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComponentDefinition {
    Form {
        id: String,
        resource: String,
        mode: FormMode,
        fields: Vec<String>,
        /// 页面简写可覆盖展示文案，不改变 Resource 的持久化约束。
        labels: BTreeMap<String, String>,
        source: SourceLocation,
    },
    Table {
        resource: String,
        columns: Vec<String>,
        source: SourceLocation,
    },
    Group {
        label: Option<String>,
        children: Vec<ComponentDefinition>,
        source: SourceLocation,
    },
    Custom {
        module: String,
        props: BTreeMap<String, PropValue>,
        children: Vec<ComponentDefinition>,
        source: SourceLocation,
    },
    Text {
        value: String,
        source: SourceLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PropValue {
    Literal(Value),
    Module(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEnvironment {
    Client,
    Server,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDefinition {
    pub name: String,
    pub runtime: ExecutionEnvironment,
    pub permission: Role,
    pub handler: String,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationDefinition {
    pub label: Option<String>,
    pub items: Vec<NavigationItem>,
    pub source: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NavigationItem {
    Link {
        label: String,
        path: String,
        permission: Role,
        source: SourceLocation,
    },
    Group {
        label: Option<String>,
        items: Vec<NavigationItem>,
        source: SourceLocation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Create,
    List,
    Read,
    Update,
    Delete,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum PermissionTarget {
    Resource(String),
    Action(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDefinition {
    pub target: PermissionTarget,
    pub operation: Operation,
    pub role: Role,
    /// 表单策略按单项匹配，不能把多个策略的字段并集合并授权。
    pub fields: Vec<String>,
    pub form: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleReferenceKind {
    ClientComponent,
    ClientFunction,
    ServerFunction,
    SharedFunction,
    ClientExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleReference {
    pub id: String,
    pub module: PathBuf,
    pub export: String,
    pub external: bool,
    pub kind: ModuleReferenceKind,
    pub expression: Option<kengine::SourceSpan>,
    pub source: SourceLocation,
}
