//! 正式交付格式 1；所有执行输入均随步骤冻结，不调用现行应用业务函数。
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::super::schema::{SCHEMA_HASH_VERSION, SchemaModel, hash};
use super::super::{MigrationError, Result};

pub const FORMAT_VERSION: u32 = 1;
pub const EXECUTOR: &str = "sqlite-schema-v1";
pub const MIN_CLI_VERSION: &str = "0.2.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Data,
    Destructive,
}

pub(super) fn risk_level(risks: &[Risk]) -> RiskLevel {
    if risks.iter().any(|risk| risk.code != "data_sql") {
        RiskLevel::Destructive
    } else if risks.is_empty() {
        RiskLevel::Safe
    } else {
        RiskLevel::Data
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub schema_hash_version: String,
    pub steps: Vec<MigrationStep>,
}
impl Default for Catalog {
    fn default() -> Self {
        Self {
            format: FORMAT_VERSION,
            schema_hash_version: SCHEMA_HASH_VERSION.into(),
            steps: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationStep {
    pub format: u32,
    pub id: String,
    pub previous: Option<String>,
    pub description: String,
    pub schema_hash_version: String,
    pub from_hash: String,
    pub to_hash: String,
    pub checksum: String,
    pub executor: String,
    pub executor_format: u32,
    pub min_cli: String,
    pub schema: SchemaModel,
    pub rules: Rules,
    pub risk_level: RiskLevel,
    pub risks: Vec<Risk>,
}
impl MigrationStep {
    pub(super) fn checksum(&self) -> Result<String> {
        hash(&(
            "kenvoy-migration-step-v1",
            self.format,
            &self.id,
            &self.previous,
            &self.description,
            &self.schema_hash_version,
            &self.from_hash,
            &self.to_hash,
            &self.executor,
            self.executor_format,
            &self.min_cli,
            &self.schema,
            &self.rules,
            self.risk_level,
            &self.risks,
        ))
    }
    pub(super) fn dangerous(&self) -> bool {
        !self.risks.is_empty()
    }
    pub(super) fn has_data_operations(&self) -> bool {
        self.rules.has_data_operations()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Risk {
    pub code: String,
    pub description: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub format: u32,
    #[serde(default)]
    pub renames: Vec<Rename>,
    #[serde(default)]
    pub backfills: Vec<Expression>,
    #[serde(default)]
    pub conversions: Vec<Expression>,
    #[serde(default)]
    pub before_sql: Vec<String>,
    #[serde(default)]
    pub after_sql: Vec<String>,
    #[serde(default)]
    pub preconditions: Vec<Assertion>,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub baseline_proofs: Vec<BaselineProof>,
    #[serde(default)]
    pub resolved_risks: Vec<String>,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            format: 1,
            renames: vec![],
            backfills: vec![],
            conversions: vec![],
            before_sql: vec![],
            after_sql: vec![],
            preconditions: vec![],
            assertions: vec![],
            baseline_proofs: vec![],
            resolved_risks: vec![],
        }
    }
}
impl Rules {
    pub(super) fn has_data_operations(&self) -> bool {
        !self.renames.is_empty()
            || !self.backfills.is_empty()
            || !self.conversions.is_empty()
            || !self.before_sql.is_empty()
            || !self.after_sql.is_empty()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    pub table: String,
    pub from: String,
    pub to: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expression {
    pub table: String,
    pub column: String,
    pub expression: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub sql: String,
    pub expected: Value,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineProof {
    pub step: String,
    pub assertions: Vec<Assertion>,
}

pub(super) fn invalid(message: impl Into<String>) -> MigrationError {
    MigrationError::InvalidSchema(message.into())
}
pub(super) fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}
