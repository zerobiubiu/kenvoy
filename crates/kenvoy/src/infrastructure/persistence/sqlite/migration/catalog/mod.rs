//! 正式应用迁移目录：冻结交付链、候选固化、顺序执行和显式旧库接入。
mod apply;
mod baseline;
mod delivery;
mod executor;
mod inspection;
mod model;
pub(super) mod records;
mod rules;
mod sql;
mod storage;
mod validate;

use super::super::schema::{SCHEMA_HASH_VERSION, SchemaModel, schema_hash, schema_model};
use super::{MigrationError, Result};
use crate::domain::model::ApplicationIR;
pub use apply::apply;
pub use baseline::baseline;
pub use inspection::{history, status};
pub use model::{Assertion, Catalog, MigrationStep, Rename, Risk, Rules};
#[cfg(test)]
pub use model::{BaselineProof, Expression, RiskLevel};
use model::{EXECUTOR, MIN_CLI_VERSION, require, risk_level};
use serde_json::{Value, json};
use std::path::Path;
pub use storage::read;

pub fn generate(
    root: &Path,
    app: &ApplicationIR,
    description: &str,
    rules: Option<&Path>,
) -> Result<Value> {
    let _lock = storage::lock(root)?;
    let catalog = read(root)?;
    let empty = SchemaModel::default();
    let previous = catalog
        .steps
        .last()
        .map(|step| &step.schema)
        .unwrap_or(&empty);
    let schema = schema_model(app)?;
    let rules: Rules = match rules {
        Some(path) => storage::decode(path)?,
        None => Rules::default(),
    };
    require(
        previous != &schema || rules.has_data_operations(),
        "Schema 未变化且未声明数据迁移，无需生成步骤",
    )?;
    let risks = validate::risks(previous, &schema, &rules)?;
    let mut step = MigrationStep {
        format: 1,
        id: format!("{:06}", catalog.steps.len() + 1),
        previous: catalog.steps.last().map(|step| step.id.clone()),
        description: description.into(),
        schema_hash_version: SCHEMA_HASH_VERSION.into(),
        from_hash: schema_hash(previous)?,
        to_hash: schema_hash(&schema)?,
        checksum: String::new(),
        executor: EXECUTOR.into(),
        executor_format: 1,
        min_cli: MIN_CLI_VERSION.into(),
        risk_level: risk_level(&risks),
        risks,
        schema,
        rules,
    };
    step.checksum = step.checksum()?;
    validate::validate_step(
        &step,
        previous,
        step.previous.as_deref(),
        catalog.steps.len() + 1,
        false,
    )?;
    validate::scratch_steps(catalog.steps.iter().chain(std::iter::once(&step)))?;
    storage::initialize(root, &catalog)?;
    storage::draft(root, &step)?;
    Ok(
        json!({"mode":"generate","status":"draft","checksum":step.checksum,"confirmation_checksum":step.checksum,"risks":step.risks,"draft":step}),
    )
}

pub fn seal(root: &Path, app: &ApplicationIR, confirmation: Option<&str>) -> Result<Value> {
    let _lock = storage::lock(root)?;
    let catalog = read(root)?;
    let step: MigrationStep = storage::decode(&root.join("migrations/draft.json"))?;
    let empty = SchemaModel::default();
    let previous = catalog
        .steps
        .last()
        .map(|step| &step.schema)
        .unwrap_or(&empty);
    validate::validate_step(
        &step,
        previous,
        catalog.steps.last().map(|step| step.id.as_str()),
        catalog.steps.len() + 1,
        true,
    )?;
    require(
        step.schema == schema_model(app)?,
        "候选目标与当前应用 Schema 不一致，请重新生成",
    )?;
    confirm(confirmation, &step.checksum, step.dangerous())?;
    validate::scratch_steps(catalog.steps.iter().chain(std::iter::once(&step)))?;
    storage::initialize(root, &catalog)?;
    storage::commit(root, &catalog, &step)?;
    Ok(json!({"mode":"seal","status":"sealed","position":position(Some(&step))?,"step":step}))
}

pub(super) fn confirm(confirmation: Option<&str>, checksum: &str, required: bool) -> Result<()> {
    if confirmation.is_some_and(|value| value != checksum) {
        return Err(MigrationError::StaleConfirmation);
    }
    if required && confirmation.is_none() {
        return Err(MigrationError::ConfirmationRequired {
            checksum: checksum.into(),
        });
    }
    Ok(())
}
pub(super) fn position(step: Option<&MigrationStep>) -> Result<Value> {
    Ok(match step {
        Some(step) => json!({"id":step.id,"checksum":step.checksum,"schema_hash":step.to_hash}),
        None => {
            json!({"id":null,"checksum":null,"schema_hash":schema_hash(&SchemaModel::default())?})
        }
    })
}

#[cfg(test)]
mod tests;
