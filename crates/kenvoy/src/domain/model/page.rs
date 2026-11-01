use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::source::SourceLocation;

use super::permission::Role;

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
    Image {
        src: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
        fit: String,
        source: SourceLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PropValue {
    Literal(Value),
    Module(String),
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
