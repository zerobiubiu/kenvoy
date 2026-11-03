//! manifest 原子替换是提交点；历史 step 仅 create_new，永不覆写。
use super::super::super::schema::SCHEMA_HASH_VERSION;
use super::super::{MigrationError, Result};
use super::{
    Catalog, MigrationStep,
    model::{FORMAT_VERSION, invalid, require},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    schema_hash_version: String,
    steps: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    file: String,
    checksum: String,
}

pub(super) fn io(path: &Path, source: std::io::Error) -> MigrationError {
    MigrationError::Io {
        path: path.into(),
        source,
    }
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path).map_err(|e| io(path, e))?;
    require(
        metadata.file_type().is_file(),
        "迁移交付文件必须是普通文件，禁止链接或目录",
    )?;
    Ok(serde_json::from_slice(
        &fs::read(path).map_err(|e| io(path, e))?,
    )?)
}
pub(super) fn new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io(path, e))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| io(path, e))
}

pub fn read(root: &Path) -> Result<Catalog> {
    let directory = root.join("migrations");
    if !directory.exists() {
        return Ok(Catalog::default());
    }
    let path = directory.join("manifest.json");
    if !path.exists() {
        let entries = fs::read_dir(&directory).map_err(|e| io(&directory, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io(&directory, e))?;
            require(
                entry.file_name() == ".catalog.lock",
                "迁移目录非空但 manifest 缺失，禁止将历史当作空链",
            )?;
        }
        return Ok(Catalog::default());
    }
    let manifest: Manifest = decode(&path)?;
    require(
        manifest.format == FORMAT_VERSION && manifest.schema_hash_version == SCHEMA_HASH_VERSION,
        "不支持的迁移 manifest 格式或哈希算法",
    )?;
    let mut catalog = Catalog {
        format: manifest.format,
        schema_hash_version: manifest.schema_hash_version,
        steps: vec![],
    };
    let mut expected = std::collections::BTreeSet::new();
    for entry in manifest.steps {
        require(
            entry.file == format!("steps/{}.json", entry.id),
            "迁移 step 文件路径不规范",
        )?;
        require(
            expected.insert(format!("{}.json", entry.id)),
            "manifest 包含重复步骤",
        )?;
        let step: MigrationStep = decode(&directory.join(&entry.file))?;
        require(
            step.id == entry.id && step.checksum == entry.checksum,
            "manifest 与独立 step 身份或 checksum 不一致",
        )?;
        catalog.steps.push(step);
    }
    let steps = directory.join("steps");
    if steps.exists() {
        for entry in fs::read_dir(&steps).map_err(|e| io(&steps, e))? {
            let entry = entry.map_err(|e| io(&steps, e))?;
            require(
                entry
                    .file_type()
                    .map_err(|e| io(&entry.path(), e))?
                    .is_file(),
                "正式 step 目录仅允许独立普通文件",
            )?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("迁移文件名不是 UTF-8"))?;
            require(
                expected.remove(&name),
                "存在 manifest 未登记的历史步骤，禁止缺失或分叉",
            )?;
        }
    }
    require(expected.is_empty(), "manifest 登记的迁移步骤文件缺失")?;
    catalog.validate()?;
    Ok(catalog)
}

pub(super) struct DirectoryLock {
    path: PathBuf,
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub(super) fn lock(root: &Path) -> Result<DirectoryLock> {
    let directory = root.join("migrations");
    fs::create_dir_all(&directory).map_err(|e| io(&directory, e))?;
    let path = directory.join(".catalog.lock");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| io(&path, e))?;
    Ok(DirectoryLock { path })
}
pub(super) fn initialize(root: &Path, catalog: &Catalog) -> Result<()> {
    let directory = root.join("migrations");
    let manifest = directory.join("manifest.json");
    if !manifest.exists() {
        new_json(&manifest, &manifest_for(catalog))?;
    }
    let steps = directory.join("steps");
    fs::create_dir_all(&steps).map_err(|e| io(&steps, e))
}
fn manifest_for(catalog: &Catalog) -> Manifest {
    Manifest {
        format: catalog.format,
        schema_hash_version: catalog.schema_hash_version.clone(),
        steps: catalog
            .steps
            .iter()
            .map(|step| Entry {
                id: step.id.clone(),
                file: format!("steps/{}.json", step.id),
                checksum: step.checksum.clone(),
            })
            .collect(),
    }
}
pub(super) fn commit(root: &Path, catalog: &Catalog, step: &MigrationStep) -> Result<()> {
    let directory = root.join("migrations");
    let history = directory.join(format!("steps/{}.json", step.id));
    new_json(&history, step)?;
    let mut manifest = manifest_for(catalog);
    manifest.steps.push(Entry {
        id: step.id.clone(),
        file: format!("steps/{}.json", step.id),
        checksum: step.checksum.clone(),
    });
    let temporary = directory.join(format!(".manifest-{}.tmp", std::process::id()));
    let result = new_json(&temporary, &manifest).and_then(|_| {
        fs::rename(&temporary, directory.join("manifest.json")).map_err(|e| io(&temporary, e))
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&history);
        return Err(error);
    }
    fs::remove_file(directory.join("draft.json"))
        .map_err(|e| io(&directory.join("draft.json"), e))?;
    Ok(())
}
pub(super) fn draft(root: &Path, step: &MigrationStep) -> Result<()> {
    let directory = root.join("migrations");
    let temporary = directory.join(format!(".draft-{}.tmp", std::process::id()));
    new_json(&temporary, step)?;
    fs::rename(&temporary, directory.join("draft.json")).map_err(|e| io(&temporary, e))
}
