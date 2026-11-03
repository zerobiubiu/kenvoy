//! 应用就绪记录与数据库迁移位置分离；日志不作为恢复迁移的事实来源。
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use super::error::{self, Result};
use crate::infrastructure::{
    artifacts::Manifest,
    persistence::sqlite::{lifecycle, migration::catalog},
    project::Project,
};

pub(super) struct Startup {
    file: File,
    path: PathBuf,
    identity: Value,
}

impl Startup {
    pub(super) fn new(
        project: &Project,
        manifest: &Manifest,
        migrations: &catalog::Catalog,
    ) -> Result<Self> {
        let path = lifecycle::sidecar(&project.database, ".kenvoy-startups.jsonl");
        lifecycle::reject_link(&path)?;
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|source| error::io(&path, source))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| super::error::Error::InvalidConfig("系统时钟早于 Unix epoch".into()))?
            .as_nanos();
        let position = if project.snapshot {
            serde_json::json!({"kind":"development_snapshot"})
        } else {
            catalog::status(&project.database, migrations)?
        };
        Ok(Self {
            file,
            path,
            identity: serde_json::json!({
                "format_version":1,"startup_id":format!("{}-{stamp}",std::process::id()),
                "project_version":manifest.project_version,"build_id":manifest.build_id,
                "cli_version":env!("CARGO_PKG_VERSION"),"database":project.database,
                "database_migration":position,"development_snapshot":project.snapshot,
            }),
        })
    }

    pub(super) fn record(&mut self, phase: &str, detail: Option<&str>) -> Result<()> {
        #[derive(serde::Serialize)]
        struct Event<'a> {
            #[serde(flatten)]
            identity: &'a Value,
            phase: &'a str,
            timestamp_unix_ms: u128,
            #[serde(skip_serializing_if = "Option::is_none")]
            error: Option<&'a str>,
        }
        let timestamp_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| super::error::Error::InvalidConfig("系统时钟早于 Unix epoch".into()))?
            .as_millis();
        let mut bytes = serde_json::to_vec(&Event {
            identity: &self.identity,
            phase,
            timestamp_unix_ms,
            error: detail,
        })?;
        bytes.push(b'\n');
        self.file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_data())
            .map_err(|source| error::io(&self.path, source))?;
        Ok(())
    }
}
