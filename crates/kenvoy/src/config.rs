//! `kenvoy.toml` 项目配置读取。
//!
//! 对应目标项目结构的 `kenvoy.toml`（类似 `Cargo.toml`）：`[project]` / `[server]` / `[build]` / `[database]`。

use std::path::Path;

use serde::Deserialize;

/// 项目配置（对应 `kenvoy.toml`），缺失字段取默认值。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct KenvoyConfig {
    /// `[project]` 段。
    #[serde(default)]
    pub project: ProjectConfig,
    /// `[server]` 段。
    #[serde(default)]
    pub server: ServerConfig,
    /// `[build]` 段。
    #[serde(default)]
    pub build: BuildConfig,
    /// `[database]` 段。
    #[serde(default)]
    pub database: DatabaseConfig,
    /// `[storage]` 段。
    #[serde(default)]
    pub storage: StorageConfig,
}

/// `[storage]` 段（S3 对象存储：文件本体存放处，元数据在 PostgreSQL）。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct StorageConfig {
    /// 存储服务端点（如 `http://127.0.0.1:9000`）。
    pub endpoint: Option<String>,
    /// 区域。
    pub region: Option<String>,
    /// 默认 Bucket。
    pub bucket: Option<String>,
    /// 访问密钥。
    pub access_key: Option<String>,
    /// 秘密密钥。
    pub secret_key: Option<String>,
}

/// `[database]` 段（PostgreSQL 18+）。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// 连接串（如 `postgres://kenvoy:kenvoy@127.0.0.1:5433/kenvoy`）。
    pub url: Option<String>,
}

/// `[project]` 段。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ProjectConfig {
    /// 项目显示名称。
    pub name: Option<String>,
    /// 项目描述。
    pub description: Option<String>,
}

/// `[server]` 段。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ServerConfig {
    /// 监听地址。
    pub host: Option<String>,
    /// 监听端口。
    pub port: Option<u16>,
}

/// `[build]` 段。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct BuildConfig {
    /// 输出目录（相对项目根）。
    pub output: Option<String>,
    /// 静态资源访问前缀。
    pub public_path: Option<String>,
}

/// 读取项目配置；文件缺失或解析失败时回退默认值并打印提示。
#[must_use]
pub fn read_config(dir: &Path) -> KenvoyConfig {
    let path = dir.join("kenvoy.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return KenvoyConfig::default();
    };
    match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("解析 {} 失败，使用默认配置：{err}", path.display());
            KenvoyConfig::default()
        }
    }
}

/// 项目名称解析：显式 `--title` > 配置 `project.name` > 目录名 > `kenvoy`。
#[must_use]
pub fn resolve_name(dir: &Path, cfg: &KenvoyConfig, title: Option<&str>) -> String {
    if let Some(t) = title {
        return t.to_string();
    }
    if let Some(n) = &cfg.project.name {
        return n.clone();
    }
    dir.file_name().map_or_else(
        || "kenvoy".to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("kenvoy-config-test-{}", std::process::id()))
    }

    #[test]
    fn reads_config() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).expect("创建临时目录失败");
        std::fs::write(
            dir.join("kenvoy.toml"),
            "[project]\nname = \"请假管理\"\n\n[server]\nport = 8080\n\n[build]\noutput = \"out\"\n\n[storage]\nendpoint = \"http://127.0.0.1:9000\"\nbucket = \"kenvoy-dev\"\n",
        )
        .expect("写入配置失败");

        let cfg = read_config(&dir);
        assert_eq!(cfg.project.name.as_deref(), Some("请假管理"));
        assert_eq!(cfg.server.port, Some(8080));
        assert_eq!(cfg.build.output.as_deref(), Some("out"));
        assert_eq!(
            cfg.storage.endpoint.as_deref(),
            Some("http://127.0.0.1:9000")
        );
        assert_eq!(cfg.storage.bucket.as_deref(), Some("kenvoy-dev"));
        assert_eq!(cfg.storage.region, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_config_defaults() {
        let dir = temp_dir();
        let cfg = read_config(&dir);
        assert_eq!(cfg.project.name, None);
        assert_eq!(cfg.server.port, None);
    }

    #[test]
    fn resolve_name_priority() {
        let cfg = KenvoyConfig {
            project: ProjectConfig {
                name: Some("配置名".into()),
                description: None,
            },
            ..KenvoyConfig::default()
        };
        assert_eq!(resolve_name(Path::new("."), &cfg, Some("标题名")), "标题名");
        assert_eq!(resolve_name(Path::new("."), &cfg, None), "配置名");
        let empty = KenvoyConfig::default();
        assert_eq!(resolve_name(Path::new("."), &empty, None), "kenvoy");
    }
}
