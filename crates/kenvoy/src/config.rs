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
    /// `[auth]` 段。
    #[serde(default)]
    pub auth: AuthConfig,
}

/// `[auth]` 段（注册模式与密码策略）。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct AuthConfig {
    /// 注册模式：`open`（公开注册，默认）/ `review`（审核注册，注册后需审核）/ `hr`（人事绑定式，关闭公开注册）。
    #[serde(default)]
    pub mode: String,
    /// 密码强度策略。
    #[serde(default)]
    pub password: PasswordConfig,
}

/// `[auth.password]` 段（密码强度校验）。
#[derive(Debug, Default, Clone, Deserialize)]
pub struct PasswordConfig {
    /// 强度档位：`none`（不校验，默认）/ `weak` / `medium` / `strong` / `custom`（按下方自定义规则）。
    #[serde(default)]
    pub strength: String,
    /// 自定义：最短长度。
    #[serde(default)]
    pub min_length: Option<usize>,
    /// 自定义：要求大写字母。
    #[serde(default)]
    pub require_upper: Option<bool>,
    /// 自定义：要求小写字母。
    #[serde(default)]
    pub require_lower: Option<bool>,
    /// 自定义：要求数字。
    #[serde(default)]
    pub require_digit: Option<bool>,
    /// 自定义：要求符号。
    #[serde(default)]
    pub require_symbol: Option<bool>,
    /// 自定义：密码不得包含用户名。
    #[serde(default)]
    pub reject_username: Option<bool>,
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
    /// 上传大小上限（MB；缺省不限制）。
    pub max_size_mb: Option<u64>,
    /// 允许的 Content-Type 白名单（支持 `image/*` 通配；空或缺省不限制）。
    #[serde(default)]
    pub allowed_types: Vec<String>,
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

/// 密码策略配置 → 引擎校验策略（`none` / `weak` / `medium` / `strong` / `custom`）。
#[must_use]
pub fn password_policy(cfg: &PasswordConfig) -> kengine::password::PasswordPolicy {
    match cfg.strength.as_str() {
        "weak" => kengine::password::PasswordPolicy::weak(),
        "medium" => kengine::password::PasswordPolicy::medium(),
        "strong" => kengine::password::PasswordPolicy::strong(),
        "custom" => kengine::password::PasswordPolicy {
            min_length: cfg.min_length,
            require_upper: cfg.require_upper.unwrap_or(false),
            require_lower: cfg.require_lower.unwrap_or(false),
            require_digit: cfg.require_digit.unwrap_or(false),
            require_symbol: cfg.require_symbol.unwrap_or(false),
            reject_username: cfg.reject_username.unwrap_or(false),
        },
        _ => kengine::password::PasswordPolicy::none(),
    }
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
