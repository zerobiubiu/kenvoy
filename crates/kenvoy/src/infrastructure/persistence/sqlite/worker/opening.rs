//! 正式链、一次性开发快照和低层测试的初始化边界互不隐式转换。
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::Duration,
};

use rusqlite::Connection;

use super::{
    super::{
        DatabaseError, Result,
        lifecycle::{self, DatabaseLease},
        migration, snapshot,
    },
    WorkerState,
};
use crate::domain::model::ApplicationIR;

pub(super) enum Opening {
    Catalog {
        catalog: migration::catalog::Catalog,
        confirmation: Option<String>,
    },
    Snapshot,
    #[cfg(test)]
    Legacy {
        confirmation: Option<String>,
    },
}

impl Opening {
    pub(super) fn validate(
        &self,
        path: &std::path::Path,
        application: &ApplicationIR,
    ) -> Result<()> {
        migration::schema_model(application)?;
        if let Self::Catalog { catalog, .. } = self {
            catalog.validate()?;
            catalog.validate_target(application)?;
            snapshot::reject(path)?;
        }
        Ok(())
    }
}

pub(super) fn initialize(
    path: PathBuf,
    application: ApplicationIR,
    opening: Opening,
) -> Result<WorkerState> {
    let strict_schema = match &opening {
        #[cfg(test)]
        Opening::Legacy { .. } => false,
        _ => true,
    };
    let lease = if strict_schema {
        Some(DatabaseLease::acquire(&path)?)
    } else {
        None
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| DatabaseError::Io {
            path: parent.into(),
            source,
        })?;
    }
    if matches!(opening, Opening::Snapshot) {
        create_snapshot(&path, &application)?;
    }
    let mut connection = Connection::open(&path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    match opening {
        Opening::Catalog {
            catalog,
            confirmation,
        } => {
            migration::catalog::apply(&mut connection, &path, &catalog, confirmation.as_deref())?;
        }
        Opening::Snapshot => {
            snapshot::mark(&mut connection, &migration::schema_model(&application)?)?;
            migration::apply_connection(&mut connection, &application, Some(&path), None)?;
        }
        #[cfg(test)]
        Opening::Legacy { confirmation } => {
            migration::apply_connection(
                &mut connection,
                &application,
                Some(&path),
                confirmation.as_deref(),
            )?;
        }
    }
    super::super::uploads::initialize(&mut connection, super::super::uploads::now()?)?;
    Ok(WorkerState {
        connection,
        path,
        application,
        generation: 1,
        strict_schema,
        _lease: lease,
    })
}

fn create_snapshot(path: &std::path::Path, application: &ApplicationIR) -> Result<()> {
    for suffix in ["", "-wal", "-shm", ".kenvoy-snapshot.json"] {
        let candidate = lifecycle::sidecar(path, suffix);
        if candidate.try_exists().map_err(|source| DatabaseError::Io {
            path: candidate.clone(),
            source,
        })? {
            return Err(DatabaseError::InvalidState(format!(
                "开发快照路径已存在，拒绝覆盖：{}",
                candidate.display()
            )));
        }
    }
    let marker = lifecycle::sidecar(path, ".kenvoy-snapshot.json");
    let model = migration::schema_model(application)?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "format_version": 1, "kind": "development_snapshot",
        "schema_hash_version": "sha256-schema-v1", "schema_hash": super::super::schema::schema_hash(&model)?,
    })).map_err(migration::MigrationError::from)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|source| DatabaseError::Io {
            path: marker.clone(),
            source,
        })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| DatabaseError::Io {
            path: marker,
            source,
        })?;
    // 标记先落盘；初始化失败也不能让剩余文件被正式运行误接管。
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| DatabaseError::Io {
            path: path.into(),
            source,
        })?;
    Ok(())
}
