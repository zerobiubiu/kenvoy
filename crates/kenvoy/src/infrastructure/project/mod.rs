//! 本地配置、项目发现与源码读取；错误不依赖启动装配层。
mod analysis;
mod compatibility;
mod config;
mod create;
pub mod editor;
mod error;
pub mod i18n;
mod paths;
mod snapshot;

#[cfg(test)]
mod compatibility_tests;

#[cfg(test)]
mod origin_tests;
#[cfg(test)]
mod regression_tests;
#[cfg(test)]
mod tests;

pub use create::create;
pub use error::{ProjectError, Result};

use std::{
    fs,
    path::{Path, PathBuf},
};

use config::Config;
use error::{ProjectError as Error, io};
use paths::{overlaps, safe_path};

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
    pub entry: PathBuf,
    pub database: PathBuf,
    pub out_dir: PathBuf,
    pub snapshot: bool,
}

impl Project {
    pub fn discover(start: &Path) -> Result<Self> {
        let start = start.canonicalize().map_err(|error| io(start, error))?;
        for directory in start.ancestors() {
            if directory.join("kenvoy.toml").is_file() {
                return Self::load(directory);
            }
        }
        Err(Error::InvalidConfig(
            "未找到 kenvoy.toml；请在项目目录中运行或先执行 kenvoy create".into(),
        ))
    }

    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|error| io(root, error))?;
        let path = root.join("kenvoy.toml");
        let source = fs::read_to_string(&path).map_err(|error| io(&path, error))?;
        let config: Config = toml::from_str(&source).map_err(|source: toml::de::Error| {
            if source.message().starts_with("unknown field `i18n`") {
                Error::InvalidConfig(format!(
                    "不再支持旧 [i18n] 配置：{source}\n请手工转换为 [languages] 与根 translations.json，保留 \"$schema\":\"./language.schema.json\" 并为每个词条补齐所有配置语言；将项目格式和最低 CLI 声明为 0.4.0 或受支持的更高版本，再执行 kenvoy i18n。既有 sealed 迁移不得改写。"
                ))
            } else {
                Error::Config { path, source }
            }
        })?;
        config.server.resolve_origin(config.server.port)?;
        config
            .languages
            .validate()
            .map_err(|error| Error::InvalidConfig(error.to_string()))?;
        if config.project.name.trim().is_empty()
            || config.project.name.chars().any(char::is_control)
        {
            return Err(Error::InvalidConfig(
                "project.name 不能为空或包含控制字符".into(),
            ));
        }
        for (key, actual, supported) in [
            (
                "database.provider",
                config.database.provider.as_str(),
                "sqlite",
            ),
            (
                "frontend.framework",
                config.frontend.framework.as_str(),
                "react",
            ),
            ("frontend.render", config.frontend.render.as_str(), "csr"),
            (
                "runtime.javascript",
                config.runtime.javascript.as_str(),
                "quickjs",
            ),
        ] {
            if actual != supported {
                return Err(Error::InvalidConfig(format!(
                    "{key} 当前仅支持 {supported}，收到 {actual}"
                )));
            }
        }
        let entry = safe_path(&root, &config.application.entry)?;
        let database = safe_path(&root, &config.database.path)?;
        let out_dir = safe_path(&root, &config.build.out_dir)?;
        let migrations = safe_path(&root, Path::new("migrations"))?;
        let source_dir = entry
            .parent()
            .ok_or_else(|| Error::InvalidConfig("入口必须有源码目录".into()))?;
        let data_dir = database
            .parent()
            .ok_or_else(|| Error::InvalidConfig("数据库必须有持久化目录".into()))?;
        if source_dir == root
            || data_dir == root
            || source_dir == data_dir
            || overlaps(source_dir, data_dir)
            || overlaps(&out_dir, source_dir)
            || overlaps(&out_dir, data_dir)
            || overlaps(&migrations, source_dir)
            || overlaps(&migrations, data_dir)
            || overlaps(&migrations, &out_dir)
            || out_dir == root
            || out_dir.starts_with(root.join(".kenvoy"))
        {
            return Err(Error::InvalidConfig(
                "源码、数据库、正式 migrations 和构建输出必须位于项目内彼此独立的目录；输出不能是项目根或 .kenvoy"
                    .into(),
            ));
        }
        let project = Self {
            root,
            config,
            entry,
            database,
            out_dir,
            snapshot: false,
        };
        Ok(project)
    }
}
