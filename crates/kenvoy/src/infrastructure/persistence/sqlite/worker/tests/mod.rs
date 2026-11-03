mod delivery;
mod lifecycle;
mod migrations;
mod options;
mod queue;
mod records;
mod snapshot_origin;
mod upload_integrity;
mod uploads;

use super::*;
use crate::application::resources::{
    FieldFilter, FilterOperator, ListQuery, NewUpload, OptionsQuery,
};
use serde_json::Value;
use serde_json::json;
use std::{
    error::Error,
    fs,
    sync::atomic::AtomicUsize,
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
static FIXTURE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    database: PathBuf,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "kenvoy-database-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root)?;
        fs::create_dir(root.join("src"))?;
        Ok(Self {
            database: root.join("data/app.sqlite"),
            root,
        })
    }
    fn application(&self, fields: &str) -> TestResult<ApplicationIR> {
        let entry = self.root.join("src/app.tsx");
        fs::write(
            &entry,
            format!(
                "export default <app name='数据库验证'><resource name='customers'>{fields}</resource></app>;"
            ),
        )?;
        let program = kengine::Engine::new().analyze_project(
            &self.root,
            kengine::AnalyzeOptions {
                entry_points: vec![entry.clone()],
                ..Default::default()
            },
        )?;
        if program.has_errors() {
            return Err(format!("解析失败：{:?}", program.diagnostics).into());
        }
        let analyzed = crate::application::compilation::analyze(
            &program,
            &entry.canonicalize()?,
            "数据库验证",
            &crate::infrastructure::javascript::SwcClientCodeTransformer,
        );
        analyzed
            .application
            .ok_or_else(|| format!("DSL 失败：{:?}", analyzed.diagnostics).into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

const BASIC: &str = "<text name='name' required maxLength={100}/><text name='phone'/><number name='age' integer min={0}/>";

fn attachment(owner: char, field: &str, name: &str) -> NewUpload {
    NewUpload {
        resource: "customers".into(),
        field: field.into(),
        owner: owner.to_string().repeat(64),
        name: name.into(),
        content_type: "application/pdf".into(),
        data: b"%PDF-1.7\nbinary".to_vec(),
        expires_at: crate::infrastructure::persistence::sqlite::uploads::now()
            .expect("测试时钟有效")
            + 3600,
    }
}

const ATTACHMENTS: &str = "<text name='name' required unique/><file name='attachments' maxFiles={2} accept={['application/pdf']}/><file name='secondary' maxFiles={2}/>";
