use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::model::FieldDefinition;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub field: Option<String>,
    pub message: String,
    pub message_key: String,
    pub params: BTreeMap<String, Value>,
}

impl ValidationError {
    pub fn new(code: &str, field: Option<String>, message: &str, message_key: &str) -> Self {
        Self {
            code: code.into(),
            field,
            message: message.into(),
            message_key: message_key.into(),
            params: BTreeMap::new(),
        }
    }

    pub fn with_param(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.params.insert(name.into(), value.into());
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{} 字段 {field}：{}", self.code, self.message),
            None => write!(f, "{}：{}", self.code, self.message),
        }
    }
}
impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationErrors(pub Vec<ValidationError>);

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, "；")?
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ValidationErrors {}

pub(super) fn field_error(
    field: &FieldDefinition,
    code: &str,
    message: &str,
    message_key: &str,
) -> ValidationError {
    ValidationError::new(code, Some(field.name.clone()), message, message_key)
}
