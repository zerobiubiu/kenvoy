use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::model::FieldDefinition;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub field: Option<String>,
    pub message: String,
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

pub(super) fn field_error(field: &FieldDefinition, code: &str, message: &str) -> ValidationError {
    ValidationError {
        code: code.into(),
        field: Some(field.name.clone()),
        message: message.into(),
    }
}
