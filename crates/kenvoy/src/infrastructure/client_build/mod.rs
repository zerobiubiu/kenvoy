//! 自定义客户端只在独占 staging 中构建；框架缓存与用户依赖安装分开管理。
mod assets;
mod build;
mod cache;
mod command;
pub mod development;
mod error;
mod graph;
mod sources;

#[cfg(test)]
mod tests;

pub use build::build;
pub use error::{ClientBuildError, Result};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClientBuildOptions {
    pub install_deps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientBuildOutput {
    pub client_dir: PathBuf,
    pub fingerprint: String,
    pub files: BTreeMap<String, ClientAsset>,
    pub dependencies: BTreeMap<String, String>,
    #[serde(skip)]
    seed: Option<DevSeed>,
}
#[derive(Debug, Clone)]
struct DevSeed {
    node: PathBuf,
    request: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAsset {
    pub size: u64,
    pub sha256: String,
}
