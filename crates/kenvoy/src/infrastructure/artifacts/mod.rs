//! 版本化产物只追加到独立目录；原子更新清单，不清理或覆盖业务数据。
mod files;
mod fingerprint;
mod load;
mod relocate;
mod staging;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod tests;

pub use fingerprint::input_fingerprint;
pub use load::{activate, load};
pub use staging::Staging;

use crate::{
    domain::{i18n::I18nBundle, model::ApplicationIR},
    infrastructure::persistence::sqlite::migration::{self, catalog::Catalog},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

const FORMAT_VERSION: u32 = 3;
const INPUT_HASH_VERSION: &str = "sha256-input-v3";
const BUILD_HASH_VERSION: &str = "sha256-build-v3";
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("构建产物 {path} 无法访问：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("构建产物无效：{0}")]
    Invalid(String),
    #[error("构建产物序列化失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("构建产物迁移链无效：{0}")]
    Migration(#[from] migration::MigrationError),
}
pub type Result<T> = std::result::Result<T, ArtifactError>;
fn io(path: impl Into<PathBuf>, source: std::io::Error) -> ArtifactError {
    ArtifactError::Io {
        path: path.into(),
        source,
    }
}
fn invalid(message: impl Into<String>) -> ArtifactError {
    ArtifactError::Invalid(message.into())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn i18n_config_hash(project: &crate::infrastructure::project::Project) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&(
        "kenvoy-i18n-config-v1",
        &project.config.i18n,
    ))?))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub project_name: String,
    pub project_version: String,
    pub min_cli_version: String,
    pub project_format_version: String,
    pub development_snapshot: bool,
    pub input_hash_version: String,
    pub build_hash_version: String,
    pub schema_hash_version: String,
    pub framework_version: String,
    pub framework_fingerprint: String,
    pub build_id: String,
    pub input_fingerprint: String,
    pub i18n_config_hash: String,
    pub files: BTreeMap<String, Asset>,
    pub dependencies: BTreeMap<String, String>,
}
impl Manifest {
    fn identity(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(&(
            self.format_version,
            &self.project_name,
            &self.project_version,
            &self.min_cli_version,
            &self.project_format_version,
            self.development_snapshot,
            &self.input_hash_version,
            &self.build_hash_version,
            &self.schema_hash_version,
            &self.framework_version,
            &self.framework_fingerprint,
            &self.input_fingerprint,
            &self.i18n_config_hash,
            &self.files,
            &self.dependencies,
        ))?))
    }
}

#[derive(Clone)]
pub struct Loaded {
    pub manifest: Manifest,
    pub application: ApplicationIR,
    pub migrations: Catalog,
    pub i18n: Arc<I18nBundle>,
    pub server_modules: Vec<u8>,
    pub client: PathBuf,
}
