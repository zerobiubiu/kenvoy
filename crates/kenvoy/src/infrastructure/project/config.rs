use std::{net::IpAddr, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{ProjectError as Error, Result};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub project: ProjectConfig,
    pub application: ApplicationConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub frontend: FrontendConfig,
    pub runtime: RuntimeConfig,
    pub build: BuildConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
}
impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "kenvoy-app".into(),
            version: "0.1.0".into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApplicationConfig {
    pub entry: PathBuf,
}
impl Default for ApplicationConfig {
    fn default() -> Self {
        Self {
            entry: "src/app.tsx".into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_origin: Option<String>,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::from([127, 0, 0, 1]),
            port: 3000,
            public_origin: None,
        }
    }
}
impl ServerConfig {
    /// 固定浏览器入口用于 Origin 和 Cookie 策略，不信任请求中的代理头。
    pub fn resolve_origin(&self, bound_port: u16) -> Result<String> {
        let Some(raw) = &self.public_origin else {
            if !self.host.is_loopback() {
                return Err(Error::InvalidConfig(
                    "非回环监听必须配置 HTTPS server.public_origin".into(),
                ));
            }
            return Ok(format!(
                "http://{}",
                std::net::SocketAddr::new(self.host, bound_port)
            ));
        };
        let url = url::Url::parse(raw).map_err(|_| {
            Error::InvalidConfig("server.public_origin 必须是有效的 HTTP(S) Origin".into())
        })?;
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
            None => false,
        };
        if raw.trim() != raw
            || raw.chars().any(char::is_control)
            || !matches!(url.scheme(), "http" | "https")
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || (url.scheme() == "http" && (!self.host.is_loopback() || !loopback))
            || url.port() == Some(0)
        {
            return Err(Error::InvalidConfig("server.public_origin 只允许无凭据、路径、查询或片段的 HTTPS Origin；HTTP 仅限回环开发地址且端口不能为 0".into()));
        }
        Ok(url.origin().ascii_serialization())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    pub provider: String,
    pub path: PathBuf,
}
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            provider: "sqlite".into(),
            path: "data/app.sqlite".into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FrontendConfig {
    pub framework: String,
    pub render: String,
}
impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            framework: "react".into(),
            render: "csr".into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub javascript: String,
    pub trust: crate::application::execution::Trust,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            javascript: "quickjs".into(),
            trust: crate::application::execution::Trust::Project,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildConfig {
    pub out_dir: PathBuf,
}
impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            out_dir: "dist".into(),
        }
    }
}
