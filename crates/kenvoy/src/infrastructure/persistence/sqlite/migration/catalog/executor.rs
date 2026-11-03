//! sqlite-schema-v1：固定建表/重建/复制语义；不调用应用 IR、业务函数或部署源码。
use super::super::super::schema::{SchemaModel, ddl, frozen, quoted};
use super::super::{MigrationError, Result};
use super::{
    MigrationStep,
    sql::{self, Gate},
};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn execute(
    connection: &Connection,
    gate: &Gate,
    previous: &SchemaModel,
    step: &MigrationStep,
) -> Result<()> {
    sql::assertions(connection, gate, &step.rules.preconditions)?;
    for sql in &step.rules.before_sql {
        user_sql(connection, gate, previous, step, sql)?;
    }
    for rule in step.rules.backfills.iter().chain(&step.rules.conversions) {
        sql::expression(connection, gate, &rule.table, &rule.expression)?;
    }
    for (name, target) in &step.schema.tables {
        let Some(old) = previous.tables.get(name) else {
            connection.execute_batch(&ddl::create_table(name, target))?;
            ddl::create_indexes(connection, name, target)?;
            continue;
        };
        let maps = step.rules.renames.iter().any(|rule| &rule.table == name)
            || step
                .rules
                .backfills
                .iter()
                .chain(&step.rules.conversions)
                .any(|rule| &rule.table == name);
        let rebuild = maps
            || old
                .columns
                .iter()
                .any(|(column, model)| target.columns.get(column) != Some(model));
        if rebuild {
            let temporary = format!("_kenvoy_rebuild_{name}");
            if super::super::state::table_exists(connection, &temporary)? {
                return Err(MigrationError::Drift("发现迁移临时表，拒绝覆盖".into()));
            }
            let sequence: i64 = connection
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name=?1",
                    [name],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);
            connection.execute_batch(&ddl::create_table(&temporary, target))?;
            let mut columns = vec!["id".into(), "created_at".into(), "updated_at".into()];
            let mut expressions: Vec<String> =
                columns.iter().map(|name: &String| quoted(name)).collect();
            for column in target.columns.keys() {
                let conversion = step
                    .rules
                    .conversions
                    .iter()
                    .find(|rule| &rule.table == name && &rule.column == column);
                let backfill = step
                    .rules
                    .backfills
                    .iter()
                    .find(|rule| &rule.table == name && &rule.column == column);
                let rename = step
                    .rules
                    .renames
                    .iter()
                    .find(|rule| &rule.table == name && &rule.to == column);
                let source = rename.map(|rule| rule.from.as_str()).unwrap_or(column);
                let expression = if let Some(rule) = conversion {
                    Some(format!("({})", rule.expression))
                } else if old.columns.contains_key(source) {
                    Some(if let Some(rule) = backfill {
                        format!("coalesce({},({}))", quoted(source), rule.expression)
                    } else {
                        quoted(source)
                    })
                } else {
                    backfill.map(|rule| format!("({})", rule.expression))
                };
                if let Some(expression) = expression {
                    columns.push(column.clone());
                    expressions.push(expression);
                }
            }
            let destination = columns
                .iter()
                .map(|name| quoted(name))
                .collect::<Vec<_>>()
                .join(",");
            gate.copy(
                connection,
                &format!(
                    "INSERT INTO {} ({destination}) SELECT {} FROM {};",
                    quoted(&temporary),
                    expressions.join(","),
                    quoted(name)
                ),
            )?;
            let before: i64 = connection.query_row(
                &format!("SELECT count(*) FROM {}", quoted(name)),
                [],
                |row| row.get(0),
            )?;
            let after: i64 = connection.query_row(
                &format!("SELECT count(*) FROM {}", quoted(&temporary)),
                [],
                |row| row.get(0),
            )?;
            if before != after {
                return Err(MigrationError::InvalidData(
                    "固定迁移重建记录数不守恒".into(),
                ));
            }
            connection.execute_batch(&format!(
                "DROP TABLE {}; ALTER TABLE {} RENAME TO {};",
                quoted(name),
                quoted(&temporary),
                quoted(name)
            ))?;
            if connection.execute(
                "UPDATE sqlite_sequence SET seq=max(seq,?1) WHERE name=?2",
                params![sequence, name],
            )? == 0
                && sequence > 0
            {
                connection.execute(
                    "INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)",
                    params![name, sequence],
                )?;
            }
            ddl::create_indexes(connection, name, target)?;
        } else {
            for (column, model) in &target.columns {
                if !old.columns.contains_key(column) {
                    connection.execute_batch(&format!(
                        "ALTER TABLE {} ADD COLUMN {};",
                        quoted(name),
                        ddl::column_sql(model)
                    ))?;
                }
            }
            for (index, model) in &old.indexes {
                if target.indexes.get(index) != Some(model) {
                    connection.execute_batch(&format!("DROP INDEX {};", quoted(index)))?;
                }
            }
            for (index, model) in &target.indexes {
                if old.indexes.get(index) != Some(model) {
                    ddl::create_index(connection, name, index, model)?;
                }
            }
        }
    }
    for name in previous
        .tables
        .keys()
        .filter(|name| !step.schema.tables.contains_key(*name))
    {
        connection.execute_batch(&format!("DROP TABLE {};", quoted(name)))?;
    }
    for sql in &step.rules.after_sql {
        user_sql(connection, gate, previous, step, sql)?;
    }
    frozen::verify(connection, &step.schema)?;
    sql::assertions(connection, gate, &step.rules.assertions)?;
    Ok(())
}

fn user_sql(
    connection: &Connection,
    gate: &Gate,
    previous: &SchemaModel,
    step: &MigrationStep,
    sql: &str,
) -> Result<()> {
    let before = host_objects(connection, previous, step)?;
    sql::batch(connection, gate, sql)?;
    if before != host_objects(connection, previous, step)? {
        return Err(MigrationError::Drift(
            "原始 SQL 试图创建或改写宿主内部对象".into(),
        ));
    }
    Ok(())
}
type HostObject = (String, String, String, Option<String>);

fn host_objects(
    connection: &Connection,
    previous: &SchemaModel,
    step: &MigrationStep,
) -> Result<Vec<HostObject>> {
    let mut statement = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE name NOT GLOB 'sqlite_*' ORDER BY type,name")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut out = vec![];
    for row in rows {
        let row = row?;
        if !previous.tables.contains_key(&row.2) && !step.schema.tables.contains_key(&row.2) {
            out.push(row);
        }
    }
    Ok(out)
}
