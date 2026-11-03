use std::path::Path;

use super::super::schema::{SchemaModel, hash, schema_hash};
use super::state::State;
use super::{MigrationPlan, Result, SchemaChange};

pub(super) fn plan(path: &Path, state: &State, target: &SchemaModel) -> Result<MigrationPlan> {
    let changes = diff(&state.model, target);
    let mut plan = MigrationPlan {
        database: path.into(),
        from_version: state.version,
        from_hash: schema_hash(&state.model)?,
        to_hash: schema_hash(target)?,
        actual_hash: hash(&state.actual)?,
        checksum: String::new(),
        destructive: changes.iter().any(SchemaChange::destructive),
        changes,
    };
    plan.checksum = plan_checksum(&plan)?;
    Ok(plan)
}

pub(super) fn plan_checksum(plan: &MigrationPlan) -> Result<String> {
    let mut plan = plan.clone();
    plan.checksum.clear();
    hash(&plan)
}

pub(super) fn diff(previous: &SchemaModel, target: &SchemaModel) -> Vec<SchemaChange> {
    let mut changes = vec![];
    for (name, table) in &target.tables {
        let Some(old) = previous.tables.get(name) else {
            changes.push(SchemaChange::CreateTable {
                table: name.clone(),
            });
            continue;
        };
        let added: Vec<_> = table
            .columns
            .keys()
            .filter(|name| !old.columns.contains_key(*name))
            .collect();
        let removed: Vec<_> = old
            .columns
            .keys()
            .filter(|name| !table.columns.contains_key(*name))
            .collect();
        if added.len() == 1 && removed.len() == 1 {
            let mut from = old.columns[removed[0]].clone();
            let to = &table.columns[added[0]];
            from.name = to.name.clone();
            if &from == to {
                changes.push(SchemaChange::RenameCandidate {
                    table: name.clone(),
                    from: removed[0].clone(),
                    to: added[0].clone(),
                });
            }
        }
        for column in added {
            let definition = &table.columns[column];
            changes.push(SchemaChange::AddColumn {
                table: name.clone(),
                column: column.clone(),
                safe: !definition.required
                    || definition
                        .default
                        .as_ref()
                        .is_some_and(|value| !value.is_null()),
            });
        }
        for column in removed {
            changes.push(SchemaChange::DropColumn {
                table: name.clone(),
                column: column.clone(),
            });
        }
        for (column, definition) in &table.columns {
            if old.columns.get(column).is_some_and(|old| old != definition) {
                changes.push(SchemaChange::ChangeColumn {
                    table: name.clone(),
                    column: column.clone(),
                });
            }
        }
        for (index, definition) in &table.indexes {
            if old.indexes.get(index) != Some(definition) {
                changes.push(SchemaChange::AddIndex {
                    table: name.clone(),
                    index: index.clone(),
                    unique: definition.unique,
                });
            }
        }
        for (index, definition) in &old.indexes {
            if table.indexes.get(index) != Some(definition) {
                changes.push(SchemaChange::DropIndex {
                    table: name.clone(),
                    index: index.clone(),
                });
            }
        }
    }
    for table in previous
        .tables
        .keys()
        .filter(|name| !target.tables.contains_key(*name))
    {
        changes.push(SchemaChange::DropTable {
            table: table.clone(),
        });
    }
    changes
}
