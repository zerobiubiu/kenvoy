//! 声明来自真实 SDK 产物；摘要清单只接管生成文件，不覆盖用户同名内容。
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::Path,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::super::error::{ProjectError as Error, Result};
use super::files;

const MANIFEST: &str = "manifest.json";
const GENERATOR: &str = "kenvoy-editor-types";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u8,
    generator: String,
    files: BTreeMap<String, Vec<String>>,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            format: 1,
            generator: GENERATOR.into(),
            files: BTreeMap::new(),
        }
    }
}

#[derive(Serialize)]
pub struct Refreshed {
    pub total: usize,
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub preserved_user_files: Vec<String>,
}

fn digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").expect("写入字符串不会失败");
    }
    encoded
}

fn declaration(root: &Path, name: &str) -> Result<()> {
    if !name.ends_with(".d.ts") {
        return Err(Error::InvalidConfig(format!(
            "编辑器清单只能包含 .d.ts 声明：{name}"
        )));
    }
    files::path(root, name)?;
    Ok(())
}

fn encode(manifest: &Manifest) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(manifest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn refresh(root: &Path, sources: &[(&str, &[u8])]) -> Result<Refreshed> {
    let original = files::read(root, MANIFEST)?;
    let mut pending: Manifest = match original.as_deref() {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|error| {
            Error::InvalidConfig(format!("编辑器生成清单无效，未覆盖 manifest.json：{error}"))
        })?,
        None => Manifest::default(),
    };
    if pending.format != 1 || pending.generator != GENERATOR {
        return Err(Error::InvalidConfig(
            "编辑器清单不是受支持的 Kenvoy 生成清单，未覆盖".into(),
        ));
    }
    let mut names = BTreeSet::new();
    for (name, hashes) in &pending.files {
        declaration(root, name)?;
        if !names.insert(name.to_lowercase())
            || hashes.is_empty()
            || hashes
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(Error::InvalidConfig(
                "编辑器清单包含重复路径或无效摘要，未覆盖".into(),
            ));
        }
    }
    names.clear();
    let mut desired = Manifest::default();
    for &(name, bytes) in sources {
        declaration(root, name)?;
        if !names.insert(name.to_lowercase()) {
            return Err(Error::InvalidConfig(format!(
                "内嵌编辑器声明路径重复：{name}"
            )));
        }
        desired.files.insert(name.into(), vec![digest(bytes)]);
    }
    if sources.is_empty() {
        return Err(Error::InvalidConfig(
            "内嵌编辑器声明为空，不能生成编辑器接线".into(),
        ));
    }
    let mut refreshed = Refreshed {
        total: sources.len(),
        created: 0,
        updated: 0,
        unchanged: 0,
        removed: 0,
        preserved_user_files: Vec::new(),
    };
    let mut before = BTreeMap::new();
    // 先检查完整文件集，再记录本次旧/新摘要，避免部分更新后无法重试。
    for name in pending.files.keys().chain(desired.files.keys()) {
        if before.contains_key(name) {
            continue;
        }
        let bytes = files::read(root, name)?;
        if let Some(bytes) = &bytes {
            let owned = pending
                .files
                .get(name)
                .is_some_and(|hashes| hashes.contains(&digest(bytes)));
            if !owned && desired.files.contains_key(name) {
                return Err(Error::InvalidConfig(format!(
                    "编辑器声明与用户文件冲突或生成后已被修改，未覆盖：.kenvoy/types/{name}；请先将自有内容移出该生成位置"
                )));
            }
            if !owned {
                refreshed.preserved_user_files.push(name.clone());
            }
        }
        before.insert(name.clone(), bytes);
    }
    for (name, hashes) in &desired.files {
        let accepted = pending.files.entry(name.clone()).or_default();
        if !accepted.contains(&hashes[0]) {
            accepted.push(hashes[0].clone());
        }
    }
    let pending_bytes = encode(&pending)?;
    if original.as_deref() != Some(pending_bytes.as_slice()) {
        files::replace(root, MANIFEST, original.as_deref(), &pending_bytes)?;
    }
    for &(name, bytes) in sources {
        let existing = before.get(name).and_then(Option::as_deref);
        if existing == Some(bytes) {
            refreshed.unchanged += 1;
        } else {
            files::replace(root, name, existing, bytes)?;
            if existing.is_some() {
                refreshed.updated += 1;
            } else {
                refreshed.created += 1;
            }
        }
    }
    for (name, existing) in &before {
        if desired.files.contains_key(name) || refreshed.preserved_user_files.contains(name) {
            continue;
        }
        if let Some(bytes) = existing {
            files::remove(root, name, bytes)?;
            refreshed.removed += 1;
        }
    }
    if pending.files != desired.files {
        let final_bytes = encode(&desired)?;
        files::replace(root, MANIFEST, Some(&pending_bytes), &final_bytes)?;
    }
    Ok(refreshed)
}
