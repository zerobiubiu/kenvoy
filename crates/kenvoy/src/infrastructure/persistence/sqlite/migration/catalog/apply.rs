//! 先核验有效交付前缀，再逐步提交；失败仅回滚本步，保留已成功步骤。
use super::super::super::snapshot;
use super::super::{
    MigrationError, Result,
    files::{backup_database, connection_is_memory},
    state,
};
use super::{
    Catalog, confirm,
    inspection::{database, plan_checksum, report},
    records::{self, Applied},
    sql::Gate,
};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
use std::path::Path;

pub fn apply(
    connection: &mut Connection,
    path: &Path,
    catalog: &Catalog,
    confirmation: Option<&str>,
) -> Result<Value> {
    catalog.validate()?;
    let path = database(path)?;
    snapshot::reject(&path)?;
    if let Some(actual) = connection.path() {
        let memory = actual.is_empty() || actual == ":memory:";
        if memory != connection_is_memory(&path)
            || (!memory && database(Path::new(actual))? != path)
        {
            return Err(MigrationError::InvalidSchema(
                "正式迁移连接与数据库路径不一致".into(),
            ));
        }
    }
    let state = state::read_state(connection)?;
    super::super::super::schema::frozen::verify(connection, &state.model)?;
    super::super::super::uploads::validate_bindings(connection, &state.model)?;
    let (records, start) = super::delivery::prefix(connection, catalog, &state)?;
    let checksum = plan_checksum(&path, &state, catalog, &records, start)?;
    let dangerous = catalog.steps[start..].iter().any(|step| step.dangerous());
    confirm(confirmation, &checksum, dangerous)?;
    if dangerous && connection_is_memory(&path) {
        return Err(MigrationError::BackupRequired);
    }
    let mut completed = vec![];
    for (index, step) in catalog.steps.iter().enumerate().skip(start) {
        let previous = state::read_state(connection)?;
        let (_, at) = super::delivery::prefix(connection, catalog, &previous)?;
        if at != index {
            return Err(MigrationError::Drift("执行期间正式迁移位置发生变化".into()));
        }
        let gate = Gate::install(connection, super::sql::tables(&previous.model, step))?;
        let result: Result<Applied> = (|| {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let locked = state::read_state(&transaction)?;
            let (_, locked_at) = super::delivery::prefix(&transaction, catalog, &locked)?;
            if locked_at != index
                || locked.version != previous.version
                || locked.actual != previous.actual
            {
                return Err(MigrationError::Drift(
                    "加锁后数据库结构或交付位置发生变化".into(),
                ));
            }
            super::super::super::uploads::validate_bindings(&transaction, &locked.model)?;
            let backup = if step.dangerous() {
                Some(backup_database(&path, locked.version + 1)?)
            } else {
                None
            };
            records::ensure(&transaction)?;
            if step.schema.tables.values().any(|table| {
                table
                    .columns
                    .values()
                    .any(|column| column.kind == crate::domain::model::FieldKind::File)
            }) {
                super::super::super::uploads::ensure_schema(&transaction)?;
            }
            super::executor::execute(&transaction, &gate, &locked.model, step)?;
            super::super::super::uploads::migrate_bindings(
                &transaction,
                &locked.model,
                &step.schema,
                &step.rules.renames,
            )?;
            super::super::super::uploads::cleanup_removed_uploads(&transaction, &step.schema)?;
            super::super::super::uploads::validate_bindings(&transaction, &step.schema)?;
            let legacy_version =
                records::append_legacy(&transaction, &locked, step, &path, backup.as_deref())?;
            let record = Applied {
                ordinal: index + 1,
                id: step.id.clone(),
                checksum: step.checksum.clone(),
                from_hash: step.from_hash.clone(),
                to_hash: step.to_hash.clone(),
                legacy_version,
                backup: backup.clone(),
                applied_at: records::timestamp(&transaction)?,
            };
            records::append(&transaction, &record)?;
            let final_state = state::read_state(&transaction)?;
            super::delivery::prefix(&transaction, catalog, &final_state)?;
            transaction.commit()?;
            Ok(record)
        })();
        let removed = Gate::remove(connection);
        match result {
            Ok(record) => {
                if let Err(error) = removed {
                    return Err(step_failure(connection, catalog, index, error, None));
                }
                completed.push(record);
            }
            Err(error) => {
                return Err(step_failure(
                    connection,
                    catalog,
                    index,
                    error,
                    removed.err(),
                ));
            }
        }
    }
    let final_state = state::read_state(connection)?;
    let (records, at) = super::delivery::prefix(connection, catalog, &final_state)?;
    let mut value = report(&path, catalog, &final_state, &records, at)?;
    value["mode"] = json!("apply");
    value["status"] = json!(if completed.is_empty() {
        "unchanged"
    } else {
        "applied"
    });
    value["steps"] = json!(completed);
    Ok(value)
}

fn step_failure(
    connection: &mut Connection,
    catalog: &Catalog,
    index: usize,
    cause: MigrationError,
    cleanup: Option<MigrationError>,
) -> MigrationError {
    let observed: Result<usize> = (|| {
        if !connection.is_autocommit() {
            return Err(MigrationError::Drift("连接仍处于未结束事务中".into()));
        }
        let transaction = connection.transaction()?;
        let state = state::read_state(&transaction)?;
        let (_, at) = super::delivery::prefix(&transaction, catalog, &state)?;
        Ok(at)
    })();
    let mut detail = match observed {
        Ok(at) => {
            let position = at
                .checked_sub(1)
                .map(|index| catalog.steps[index].id.as_str())
                .unwrap_or("空链");
            let outcome = if at > index {
                "本步已登记提交，不能声称回滚"
            } else if at == index {
                "失败步骤未登记成功，前序已提交步骤仍保留"
            } else {
                "交付位置低于开始本步前的位置，必须检查外部恢复或改写"
            };
            format!(
                "已核验实际交付位置 {position}；{outcome}；请用 migrate status/history 核对后继续"
            )
        }
        Err(error) => format!(
            "真实提交位置无法核验，状态未知（{error}）；不要假定全部回滚，请用 migrate status/history 检查"
        ),
    };
    if let Some(error) = cleanup {
        detail.push_str(&format!("；SQL 授权器清理也失败：{error}"));
    }
    MigrationError::CatalogStepFailed {
        step: catalog.steps[index].id.clone(),
        state: detail,
        source: Box::new(cause),
    }
}
