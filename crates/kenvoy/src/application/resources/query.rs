use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
