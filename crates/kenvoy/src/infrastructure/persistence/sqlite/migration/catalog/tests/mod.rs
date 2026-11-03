mod baseline;
mod chain;
mod mapping;
mod security;

use super::*;
use crate::domain::{
    model::{
        APPLICATION_IR_VERSION, ApplicationIR, FieldDefinition, FieldKind, ResourceDefinition,
    },
    source::SourceLocation,
};
use kengine::{Position, SourceSpan};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Fixture {
    root: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root =
            std::env::temp_dir().join(format!("kenvoy-catalog-{}-{stamp}", std::process::id()));
        fs::create_dir(&root)?;
        Ok(Self {
            path: root.join("app.sqlite"),
            root,
        })
    }
    fn seal(&self, app: &ApplicationIR, rules: Option<&Rules>) -> TestResult<Catalog> {
        let path = self.root.join("rules.json");
        if let Some(rules) = rules {
            fs::write(&path, serde_json::to_vec(rules)?)?;
        }
        let draft = generate(&self.root, app, "测试迁移", rules.map(|_| path.as_path()))?;
        seal(&self.root, app, draft["checksum"].as_str())?;
        Ok(read(&self.root)?)
    }
    fn connect(&self) -> TestResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        Ok(connection)
    }
    fn apply(&self, connection: &mut Connection, catalog: &Catalog) -> TestResult<Value> {
        let plan = status(&self.path, catalog)?;
        Ok(apply(
            connection,
            &self.path,
            catalog,
            plan["confirmation_checksum"].as_str(),
        )?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
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
        upload: None,
        source: source(),
    }
}
fn app() -> ApplicationIR {
    ApplicationIR {
        format_version: APPLICATION_IR_VERSION,
        name: "迁移测试".into(),
        resources: vec![ResourceDefinition {
            name: "items".into(),
            fields: vec![
                field("name", FieldKind::Text),
                field("age", FieldKind::Number),
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
fn assertion(sql: &str, expected: i64) -> Assertion {
    Assertion {
        sql: sql.into(),
        expected: json!(expected),
        description: "测试数据语义".into(),
    }
}
