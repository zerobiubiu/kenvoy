//! 迁移文件路径和有界一致性备份。

use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OpenFlags};

use super::{MigrationError, Result};

fn io(path: &Path, source: std::io::Error) -> MigrationError {
    MigrationError::Io {
        path: path.into(),
        source,
    }
}

pub(super) fn backup_database(path: &Path, version: u64) -> Result<PathBuf> {
    let parent = path.parent().ok_or(MigrationError::BackupRequired)?;
    let directory = parent.join("backups");
    fs::create_dir_all(&directory).map_err(|error| io(&directory, error))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MigrationError::BackupRequired)?
        .as_nanos();
    let destination = directory.join(format!(
        "before-{version}-{stamp}-{}.sqlite",
        std::process::id()
    ));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| io(&destination, error))?;
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut output = Connection::open(&destination)?;
    let backup = rusqlite::backup::Backup::new(&source, &mut output)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() >= deadline {
            return Err(MigrationError::BackupTimeout);
        }
        match backup.step(256)? {
            rusqlite::backup::StepResult::Done => break,
            rusqlite::backup::StepResult::More => {}
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    drop(backup);
    let integrity: String = output.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(MigrationError::InvalidData("备份完整性校验未通过".into()));
    }
    Ok(destination)
}

pub(super) fn connection_is_memory(path: &Path) -> bool {
    path == Path::new(":memory:")
}

pub(super) fn absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| io(path, error))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.exists() {
        return normalized
            .canonicalize()
            .map_err(|error| io(&normalized, error));
    }
    let mut existing = normalized.clone();
    let mut missing = vec![];
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        if !existing.pop() {
            return Err(MigrationError::InvalidSchema(
                "无法解析数据库绝对路径".into(),
            ));
        }
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| io(&existing, error))?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}
