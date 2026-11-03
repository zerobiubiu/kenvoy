//! 旧库接入先只读证明；精确确认后备份并原子登记来源，不伪造跳过步骤执行。
use super::super::super::schema::{frozen, schema_hash};
use super::super::super::snapshot;
use super::super::{
    MigrationError, Result,
    files::{absolute_path, backup_database},
    state::{self, State},
};
use super::{
    Catalog, Rules, confirm,
    model::require,
    records::{self, Baseline},
    sql::{self, Gate},
    storage,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub fn baseline(
    path: &Path,
    catalog: &Catalog,
    id: &str,
    rules: Option<&Path>,
    confirmation: Option<&str>,
) -> Result<Value> {
    catalog.validate()?;
    let path = absolute_path(path)?;
    snapshot::reject(&path)?;
    require(
        path.exists(),
        "真正新空库不能 baseline；请执行完整正式迁移链",
    )?;
    let at = catalog
        .steps
        .iter()
        .position(|step| step.id == id)
        .ok_or_else(|| MigrationError::InvalidSchema("baseline 指定的迁移 ID 不存在".into()))?;
    let rules: Rules = match rules {
        Some(path) => storage::decode(path)?,
        None => Rules::default(),
    };
    require(
        rules.format == 1 && !rules.has_data_operations() && rules.resolved_risks.is_empty(),
        "baseline 规则仅允许只读断言和历史数据语义证明，不能执行转换或处理迁移风险",
    )?;
    super::rules::validate(&catalog.steps[at].schema, &catalog.steps[at].schema, &rules)?;
    let mut source = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let gate = Gate::install(
        &mut source,
        catalog.steps[at].schema.tables.keys().cloned().collect(),
    )?;
    let preview = {
        let transaction = source.transaction()?;
        inspect(&transaction, &gate, catalog, at, &rules)?
    };
    Gate::remove(&mut source)?;
    drop(source);
    let mut output = report(&preview, catalog, at)?;
    if confirmation.is_none() {
        return Ok(output);
    }
    confirm(confirmation, &preview.checksum, true)?;
    let mut connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let gate = Gate::install(
        &mut connection,
        catalog.steps[at].schema.tables.keys().cloned().collect(),
    )?;
    let result: Result<Baseline> = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = inspect(&transaction, &gate, catalog, at, &rules)?;
        confirm(confirmation, &record.checksum, true)?;
        record.backup = Some(backup_database(
            &path,
            record.legacy_history.len() as u64 + 1,
        )?);
        record.created_at = records::timestamp(&transaction)?;
        records::ensure(&transaction)?;
        records::write_baseline(&transaction, &record)?;
        if !state::table_exists(&transaction, "_kenvoy_schema")? {
            records::write_snapshot(&transaction, &record.source_schema, 0)?;
        }
        let adopted = state::read_state(&transaction)?;
        super::delivery::prefix(&transaction, catalog, &adopted)?;
        transaction.commit()?;
        Ok(record)
    })();
    let removed = Gate::remove(&mut connection);
    let record = result?;
    removed?;
    output["status"] = json!("baselined");
    output["baseline"] = json!(record);
    Ok(output)
}

fn inspect(
    connection: &Connection,
    gate: &Gate,
    catalog: &Catalog,
    at: usize,
    rules: &Rules,
) -> Result<Baseline> {
    require(
        !records::verify_owner(connection)?,
        "数据库已经包含正式交付所有权，不能重新 baseline 覆盖真实历史",
    )?;
    let target = &catalog.steps[at];
    let state = match state::read_state(connection) {
        Ok(state) => state,
        Err(MigrationError::Unmanaged(_)) => {
            frozen::verify(connection, &target.schema)?;
            State {
                version: 0,
                model: target.schema.clone(),
                actual: state::actual_objects(connection)?,
            }
        }
        Err(error) => return Err(error),
    };
    let legacy = state::history(connection)?;
    require(
        !state.actual.is_empty() || !legacy.is_empty(),
        "真正新空库不能 baseline；请执行完整正式迁移链",
    )?;
    require(
        state.model == target.schema,
        "旧库 Schema 与指定 baseline 的冻结目标不一致",
    )?;
    frozen::verify(connection, &target.schema)?;
    super::super::super::uploads::validate_bindings(connection, &target.schema)?;
    let mut proofs = std::collections::BTreeSet::new();
    for proof in &rules.baseline_proofs {
        require(
            proofs.insert(&proof.step)
                && catalog.steps[..=at]
                    .iter()
                    .any(|step| step.id == proof.step),
            "baseline 证明包含重复或非跳过步骤 ID",
        )?;
        require(
            !proof.assertions.is_empty(),
            "baseline 数据语义证明必须包含必要断言",
        )?;
        sql::assertions(connection, gate, &proof.assertions)?;
    }
    for step in &catalog.steps[..=at] {
        if step.has_data_operations() {
            require(
                rules
                    .baseline_proofs
                    .iter()
                    .any(|proof| proof.step == step.id && !proof.assertions.is_empty()),
                "跳过含数据操作的历史步骤必须提供 baseline_proofs 必要断言，不能仅凭 Schema 相等接入",
            )?;
        }
    }
    sql::assertions(connection, gate, &rules.preconditions)?;
    sql::assertions(connection, gate, &rules.assertions)?;
    let mut record = Baseline {
        at_id: target.id.clone(),
        at_checksum: target.checksum.clone(),
        ordinal: at + 1,
        schema_hash: schema_hash(&state.model)?,
        skipped_checksums: catalog.steps[..=at]
            .iter()
            .map(|step| step.checksum.clone())
            .collect(),
        source_schema: state.model,
        legacy_history: legacy,
        actual_objects: state.actual,
        proofs: serde_json::to_value(rules)?,
        checksum: String::new(),
        backup: None,
        created_at: String::new(),
    };
    record.checksum = record.checksum()?;
    Ok(record)
}
fn report(record: &Baseline, catalog: &Catalog, at: usize) -> Result<Value> {
    Ok(
        json!({"mode":"baseline","status":"preview","checksum":record.checksum,"confirmation_checksum":record.checksum,
        "source_schema":record.source_schema,"legacy_history":record.legacy_history,"actual_objects":record.actual_objects,
        "target_baseline":super::position(Some(&catalog.steps[at]))?,"proofs":record.proofs,"baseline":record}),
    )
}
