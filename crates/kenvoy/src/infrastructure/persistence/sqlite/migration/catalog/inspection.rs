//! 只读一致快照、迁移计划摘要和独立审计入口。
use super::super::super::schema::{SchemaModel, hash, schema_hash};
use super::super::super::snapshot;
use super::super::{
    Result,
    files::{absolute_path, connection_is_memory},
    state::{self, State},
};
use super::{
    Catalog, position,
    records::{self, Records},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
pub(super) fn database(path: &Path) -> Result<PathBuf> {
    if connection_is_memory(path) {
        Ok(path.into())
    } else {
        absolute_path(path)
    }
}
fn empty() -> State {
    State {
        version: 0,
        model: SchemaModel::default(),
        actual: vec![],
    }
}
pub(super) fn plan_checksum(
    path: &Path,
    state: &State,
    catalog: &Catalog,
    records: &Records,
    at: usize,
) -> Result<String> {
    hash(&(
        "kenvoy-catalog-apply-v1",
        path,
        state.version,
        schema_hash(&state.model)?,
        hash(&state.actual)?,
        records.baseline.as_ref().map(|baseline| &baseline.checksum),
        &catalog.steps[at..]
            .iter()
            .map(|step| &step.checksum)
            .collect::<Vec<_>>(),
    ))
}
pub(super) fn report(
    path: &Path,
    catalog: &Catalog,
    state: &State,
    records: &Records,
    at: usize,
) -> Result<Value> {
    let pending: Vec<_> = catalog.steps[at..].iter().map(|step| &step.id).collect();
    let risks: Vec<_> = catalog.steps[at..]
        .iter()
        .flat_map(|step| &step.risks)
        .collect();
    Ok(
        json!({"mode":"status","status":if pending.is_empty(){"current"}else{"pending"},"database":path,"position":position(at.checked_sub(1).map(|index| &catalog.steps[index]))?,"pending":pending,"risks":risks,"confirmation_checksum":plan_checksum(path,state,catalog,records,at)?,"applied":records.applied,"baseline":records.baseline}),
    )
}

pub fn status(path: &Path, catalog: &Catalog) -> Result<Value> {
    catalog.validate()?;
    let path = database(path)?;
    snapshot::reject(&path)?;
    if !path.exists() {
        return report(&path, catalog, &empty(), &Records::default(), 0);
    }
    let mut source = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let connection = source.transaction()?;
    let state = state::read_state(&connection)?;
    super::super::super::schema::frozen::verify(&connection, &state.model)?;
    super::super::super::uploads::validate_bindings(&connection, &state.model)?;
    let records = records::read(&connection)?;
    if records.applied.is_empty()
        && records.baseline.is_none()
        && (state.version > 0 || !state.model.tables.is_empty())
    {
        return Ok(
            json!({"mode":"status","status":"baseline_required","database":path,"position":{"id":null,"checksum":null,"schema_hash":schema_hash(&state.model)?},"pending":catalog.steps.iter().map(|step| &step.id).collect::<Vec<_>>(),"confirmation_checksum":null,"applied":[],"baseline":null,"legacy_history":state::history(&connection)?}),
        );
    }
    let (records, at) = super::delivery::prefix(&connection, catalog, &state)?;
    report(&path, catalog, &state, &records, at)
}
/// 审计入口不依赖源码、配置兼容声明或当前交付文件。
pub fn history(path: &Path) -> Result<Value> {
    let path = database(path)?;
    if !path.exists() {
        return Ok(json!({"mode":"history","applied":[],"baseline":null,"legacy_history":[]}));
    }
    let mut source = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let connection = source.transaction()?;
    let state = state::read_state(&connection)?;
    let records = records::read(&connection)?;
    super::delivery::audit(&connection, &records, &state)?;
    Ok(
        json!({"mode":"history","applied":records.applied,"baseline":records.baseline,"legacy_history":state::history(&connection)?}),
    )
}
