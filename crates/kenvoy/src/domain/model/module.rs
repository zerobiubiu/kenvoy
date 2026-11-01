use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::source::SourceLocation;

use super::permission::Role;

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
