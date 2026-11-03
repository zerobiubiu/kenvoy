mod files;
mod rollback;
mod schema;
mod workflow;

use super::super::schema::{quote_identifier, schema_hash};
use super::state::{history, table_exists};
use super::*;
use crate::domain::{
    model::{APPLICATION_IR_VERSION, FieldDefinition, FileConstraints, ResourceDefinition},
    source::SourceLocation,
};
use kengine::{Position, SourceSpan};
use rusqlite::OpenFlags;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
struct DatabaseFile {
    directory: PathBuf,
    path: PathBuf,
}
impl DatabaseFile {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let directory =
            std::env::temp_dir().join(format!("kenvoy-migration-{}-{stamp}", std::process::id()));
        fs::create_dir(&directory)?;
        Ok(Self {
            path: directory.join("app.sqlite"),
            directory,
        })
    }
    fn connect(&self) -> TestResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }
    fn apply(&self, connection: &mut Connection, app: &ApplicationIR) -> Result<MigrationOutcome> {
        apply_connection(connection, app, Some(&self.path), None)
    }
    fn confirm(
        &self,
        connection: &mut Connection,
        app: &ApplicationIR,
    ) -> Result<MigrationOutcome> {
        let checksum = preview(&self.path, app)?.checksum;
        apply_connection(connection, app, Some(&self.path), Some(&checksum))
    }
}
impl Drop for DatabaseFile {
    fn drop(&mut self) {
        if self.directory.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}
fn source() -> SourceLocation {
    SourceLocation {
        path: PathBuf::from("fixture.tsx"),
        span: SourceSpan {
            start: 0,
            end: 1,
            start_position: Position { line: 1, column: 1 },
            end_position: Position { line: 1, column: 2 },
        },
    }
}
fn field(name: &str, kind: FieldKind) -> FieldDefinition {
    FieldDefinition {
        name: name.into(),
        label: name.into(),
        kind,
        required: false,
        default: None,
        integer: kind == FieldKind::Number,
        min: None,
        max: None,
        max_length: None,
        unique: false,
        index: false,
        options: vec![],
        options_source: None,
        upload: (kind == FieldKind::File).then(FileConstraints::default),
        source: source(),
    }
}
fn app() -> ApplicationIR {
    let mut name = field("name", FieldKind::Text);
    name.required = true;
    name.max_length = Some(100);
    ApplicationIR {
        format_version: APPLICATION_IR_VERSION,
        name: "测试".into(),
        resources: vec![ResourceDefinition {
            name: "customers".into(),
            fields: vec![
                name,
                field("age", FieldKind::Number),
                field("phone", FieldKind::Text),
            ],
            source: source(),
        }],
        pages: vec![],
        actions: vec![],
        navigation: vec![],
        permissions: vec![],
        module_references: vec![],
        component_overrides: Default::default(),
    }
}
fn rows(connection: &Connection) -> TestResult<i64> {
    Ok(connection.query_row("SELECT count(*) FROM customers", [], |row| row.get(0))?)
}
