use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    net::IpAddr,
    path::{Component, Path, PathBuf},
};

use kengine::{AnalysisSession, AnalysisUpdate, AnalyzeOptions, ResolveOptions, ResolvedModule};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, io};

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
    pub trust: crate::javascript::Trust,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            javascript: "quickjs".into(),
            trust: crate::javascript::Trust::Project,
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

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
    pub entry: PathBuf,
    pub database: PathBuf,
    pub out_dir: PathBuf,
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
        let config: Config =
            toml::from_str(&source).map_err(|source| Error::Config { path, source })?;
        config.server.resolve_origin(config.server.port)?;
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
            || out_dir == root
            || out_dir.starts_with(root.join(".kenvoy"))
        {
            return Err(Error::InvalidConfig(
                "源码、数据库和构建输出必须位于项目内彼此独立的目录；输出不能是项目根或 .kenvoy"
                    .into(),
            ));
        }
        Ok(Self {
            root,
            config,
            entry,
            database,
            out_dir,
        })
    }

    pub fn analyze(&self, session: &mut AnalysisSession) -> Result<AnalysisUpdate> {
        let packages = self.external_packages()?;
        let mut options = AnalyzeOptions {
            entry_points: vec![self.entry.clone()],
            resolve: ResolveOptions {
                externals: packages.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        if let Some(previous) = session.last_successful() {
            let resolver = ResolveOptions {
                root: self.root.clone(),
                ..Default::default()
            };
            for edge in &previous.module_graph.edges {
                if matches!(edge.target, Some(ResolvedModule::External { .. })) {
                    let package = !edge.specifier.starts_with('.')
                        && packages.iter().any(|package| {
                            edge.specifier == *package
                                || edge
                                    .specifier
                                    .strip_prefix(package)
                                    .is_some_and(|tail| tail.starts_with('/'))
                        });
                    let stylesheet = edge.specifier.starts_with('.')
                        && matches!(kengine::Engine::new().resolve_module(&edge.importer, &edge.specifier, &resolver),
                        Ok(ResolvedModule::File { path }) if path.extension().is_some_and(|extension| extension == "css"));
                    if package || stylesheet {
                        options.resolve.externals.insert(edge.specifier.clone());
                    }
                }
            }
        }
        let initial_externals = options.resolve.externals.clone();
        let first = session.analyze_project(&self.root, options.clone())?;
        for edge in &first.program.module_graph.edges {
            let package = !edge.specifier.starts_with('.')
                && packages.iter().any(|package| {
                    edge.specifier == *package
                        || edge
                            .specifier
                            .strip_prefix(package)
                            .is_some_and(|tail| tail.starts_with('/'))
                });
            let stylesheet = matches!(&edge.target, Some(ResolvedModule::File { path }) if path.extension().is_some_and(|extension| extension == "css"));
            if package || stylesheet {
                options.resolve.externals.insert(edge.specifier.clone());
            }
        }
        if options.resolve.externals == initial_externals {
            self.validate_external_stylesheets(&first)?;
            return Ok(first);
        }
        // 第二遍复用第一遍缓存，不为 external 发现重新解析所有源码。
        let mut final_update = session.analyze_project(&self.root, options)?;
        final_update.stats.parsed.extend(first.stats.parsed);
        final_update.stats.affected.extend(first.stats.affected);
        final_update.stats.evicted.extend(first.stats.evicted);
        for paths in [
            &mut final_update.stats.parsed,
            &mut final_update.stats.affected,
            &mut final_update.stats.evicted,
        ] {
            paths.sort();
            paths.dedup();
        }
        final_update
            .stats
            .reused
            .retain(|path| !final_update.stats.parsed.contains(path));
        self.validate_external_stylesheets(&final_update)?;
        Ok(final_update)
    }

    /// 相对 external 名称只是一项解析提示。必须按每个 importer 重新校验，
    /// 避免一个目录下的合法 CSS 放行另一个目录中同名但缺失或越界的文件。
    fn validate_external_stylesheets(&self, update: &AnalysisUpdate) -> Result<()> {
        let resolver = ResolveOptions {
            root: self.root.clone(),
            ..Default::default()
        };
        let engine = kengine::Engine::new();
        for edge in &update.program.module_graph.edges {
            if !edge.specifier.starts_with('.')
                || !matches!(edge.target, Some(ResolvedModule::External { .. }))
            {
                continue;
            }
            let result = engine.resolve_module(&edge.importer, &edge.specifier, &resolver);
            if !matches!(result, Ok(ResolvedModule::File { ref path }) if path.extension().is_some_and(|extension| extension == "css"))
            {
                let reason = match result {
                    Err(error) => error.to_string(),
                    _ => "相对 external 只能指向有效 CSS 文件".into(),
                };
                let mut diagnostic = crate::dsl::DslDiagnostic::new(
                    "KV3006",
                    crate::dsl::SourceLocation {
                        path: edge.importer.clone(),
                        span: edge.span.clone(),
                    },
                    format!("样式模块不能加载：{reason}"),
                    "检查当前导入文件对应的 CSS 路径，样式文件必须存在于项目根内",
                );
                diagnostic.category = crate::dsl::DslDiagnosticCategory::Module;
                return Err(Error::Dsl(vec![diagnostic]));
            }
        }
        Ok(())
    }

    /// npm 和样式解析是宿主责任，KEngine 只收到精确 external 名称。
    /// 此处只建立允许集合，是否真正可打包在构建阶段验证。
    pub(crate) fn external_packages(&self) -> Result<BTreeSet<String>> {
        let mut packages: BTreeSet<String> = [
            "@kenvoy/client",
            "react",
            "react-dom",
            "react-router",
            "@mui/material",
            "@emotion/react",
            "@emotion/styled",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let package_path = self.root.join("package.json");
        if package_path.is_file() {
            let data = fs::read(&package_path).map_err(|error| io(&package_path, error))?;
            let manifest: serde_json::Value = serde_json::from_slice(&data)?;
            for key in ["dependencies", "devDependencies", "peerDependencies"] {
                if let Some(entries) = manifest.get(key).and_then(serde_json::Value::as_object) {
                    for name in entries.keys() {
                        if !valid_package_name(name) {
                            return Err(Error::InvalidConfig(format!(
                                "package.json 中的包名无效：{name}"
                            )));
                        }
                        packages.insert(name.clone());
                    }
                }
            }
        }
        Ok(packages)
    }
}

fn overlaps(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        // 不存在的路径尾部尚不能 canonicalize，仍需遵循 Windows 文件名大小写规则。
        let left: Vec<_> = left
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        let right: Vec<_> = right
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        left.starts_with(&right) || right.starts_with(&left)
    }
    #[cfg(not(windows))]
    {
        left.starts_with(right) || right.starts_with(left)
    }
}

fn valid_package_name(name: &str) -> bool {
    fn segment(value: &str) -> bool {
        !value.is_empty()
            && !value.starts_with(['.', '_'])
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    }
    if let Some(scoped) = name.strip_prefix('@') {
        scoped
            .split_once('/')
            .is_some_and(|(scope, package)| segment(scope) && segment(package))
    } else {
        segment(name)
    }
}

/// 对尚不存在的目标，解析最近的已存在祖先后再拼接，避免链接越过项目根。
pub fn safe_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::InvalidConfig(format!(
            "路径必须是项目内相对路径且不含 ..：{}",
            relative.display()
        )));
    }
    for component in relative.components() {
        if let Component::Normal(segment) = component {
            let Some(segment) = segment.to_str() else {
                return Err(Error::InvalidConfig("项目路径必须为有效 Unicode".into()));
            };
            let stem = segment
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let reserved = matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || ["COM", "LPT"].iter().any(|prefix| {
                stem.strip_prefix(prefix).is_some_and(|number| {
                    (number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'))
                        || matches!(number, "¹" | "²" | "³")
                })
            });
            if segment
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                || segment.ends_with(['.', ' '])
                || reserved
                || [".git", ".hg", ".svn", "node_modules", ".kenvoy"]
                    .iter()
                    .any(|blocked| segment.eq_ignore_ascii_case(blocked))
            {
                return Err(Error::InvalidConfig(format!(
                    "项目路径包含保留目录、设备名或不可移植组件：{segment}"
                )));
            }
        }
    }
    let joined = root.join(relative);
    let mut ancestor = joined.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(
            ancestor
                .file_name()
                .ok_or_else(|| Error::InvalidConfig("无法解析项目路径".into()))?
                .to_owned(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| Error::InvalidConfig("无法解析项目路径祖先".into()))?;
    }
    let mut resolved = ancestor
        .canonicalize()
        .map_err(|error| io(ancestor, error))?;
    for segment in suffix.iter().rev() {
        resolved.push(segment);
    }
    if !resolved.starts_with(root) {
        return Err(Error::InvalidConfig(format!(
            "路径超出项目根：{}",
            relative.display()
        )));
    }
    Ok(resolved)
}

pub const CUSTOMER_APP: &str = r#"export default (
  <app name="客户管理系统">
    <resource name="customers">
      <text name="name" label="客户姓名" required maxLength={100} />
      <text name="phone" label="联系电话" />
      <number name="age" label="年龄" integer min={0} />
    </resource>
    <page path="/customers" title="客户登记">
      <form resource="customers" mode="create" />
    </page>
  </app>
);
"#;

#[cfg(test)]
mod origin_tests {
    use super::*;

    #[test]
    fn deployment_origin_is_fixed_and_https_is_required_outside_loopback() -> Result<()> {
        let mut config = ServerConfig::default();
        assert_eq!(config.resolve_origin(41234)?, "http://127.0.0.1:41234");
        config.host = "::1"
            .parse()
            .map_err(|_| Error::InvalidConfig("测试地址无效".into()))?;
        assert_eq!(config.resolve_origin(41234)?, "http://[::1]:41234");
        for invalid in [
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com?q=1",
            "https://example.com#x",
            "ftp://example.com",
            "http://example.com",
            "http://127.0.0.1:0",
            " https://example.com",
        ] {
            config.public_origin = Some(invalid.into());
            assert!(
                config.resolve_origin(3000).is_err(),
                "错误配置被接受：{invalid}"
            );
        }
        config.public_origin = Some("http://localhost:3000".into());
        assert_eq!(config.resolve_origin(3000)?, "http://localhost:3000");
        config.host = "0.0.0.0"
            .parse()
            .map_err(|_| Error::InvalidConfig("测试地址无效".into()))?;
        assert!(config.resolve_origin(3000).is_err());
        config.public_origin = None;
        assert!(config.resolve_origin(3000).is_err());
        config.public_origin = Some("https://app.example.com:443/".into());
        assert_eq!(config.resolve_origin(3000)?, "https://app.example.com");
        Ok(())
    }
}

pub fn create(target: &Path) -> Result<()> {
    if target.exists() {
        return Err(Error::InvalidConfig(format!(
            "目标已存在，不能覆盖：{}",
            target.display()
        )));
    }
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .ok_or_else(|| Error::InvalidConfig("项目名称无效".into()))?;
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|error| io(parent, error))?;
    let target = parent.join(name);
    let mut config = Config::default();
    config.project.name = name.to_owned();
    let encoded =
        toml::to_string_pretty(&config).map_err(|error| Error::InvalidConfig(error.to_string()))?;
    // 原子保留目标目录，跨平台都不会用 rename 替换已存在的空目录。
    // 中途失败保留本次部分文件供排查，不删除未知内容或再次覆盖。
    fs::create_dir(&target).map_err(|error| io(&target, error))?;
    for directory in ["src", "data"] {
        let path = target.join(directory);
        fs::create_dir(&path).map_err(|error| io(path, error))?;
    }
    for (path, content) in [
        ("kenvoy.toml", encoded.as_str()),
        ("src/app.tsx", CUSTOMER_APP),
        (
            ".gitignore",
            "data/*.sqlite*\ndist/\n.kenvoy/\nnode_modules/\n.env*\n",
        ),
        (
            "README.md",
            "# Kenvoy 应用\n\n执行 `kenvoy dev` 启动，访问 http://127.0.0.1:3000/customers。\n\n通过 `kenvoy admin init --username admin` 隐藏输入密码初始化管理员，再访问 /admin/customers；没有默认密码。\n\n`kenvoy check` 检查声明，`kenvoy inspect customers` 查看资源，`kenvoy migrate status` 预览迁移。\n\n`kenvoy build` 准备产物；`kenvoy run` 会按需自动构建并启动生产服务，不需要事先执行 build。自定义组件的 npm 依赖已在 package.json 声明时，首次可明确使用 `kenvoy dev --install-deps`。\n\n源码入口为 src/app.tsx，SQLite 保存在 data/app.sqlite；构建不会清理数据。\n",
        ),
    ] {
        let path = target.join(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| io(&path, error))?;
        file.write_all(content.as_bytes())
            .map_err(|error| io(&path, error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_configuration_and_unsupported_scalar_types_are_rejected() {
        assert!(toml::from_str::<Config>("[server]\nporrt=3000").is_err());
        assert!(toml::from_str::<Config>("[server]\nport='3000'").is_err());
        assert!(toml::from_str::<Config>("[unknown]\nvalue=1").is_err());
        assert_eq!(
            toml::from_str::<Config>("").expect("默认配置").server.port,
            3000
        );
    }
    #[test]
    fn paths_reject_parent_absolute_and_overlapping_boundaries() -> Result<()> {
        let root = std::env::current_dir()
            .map_err(|error| io(".", error))?
            .canonicalize()
            .map_err(|error| io(".", error))?;
        assert!(safe_path(&root, Path::new("../outside")).is_err());
        assert!(safe_path(&root, &root).is_err());
        assert!(overlaps(&root.join("src"), &root.join("src/dist")));
        assert!(!overlaps(&root.join("src"), &root.join("src-other")));
        Ok(())
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let path =
                std::env::temp_dir().join(format!("kenvoy-project-{}-{stamp}", std::process::id()));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
        fn write(
            &self,
            name: &str,
            value: &str,
        ) -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?
            }
            fs::write(&path, value)?;
            Ok(path.canonicalize()?)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            if self.0.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn stylesheet_noop_and_changes_reuse_code_while_ui_changes_parse_one_module()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let scratch = Scratch::new()?;
        scratch.write("kenvoy.toml", "")?;
        scratch.write(
            "src/app.tsx",
            "import { Widget } from './Widget'; export default <Widget />;",
        )?;
        let widget = scratch.write("src/Widget.tsx", "import './style.css'; import { Button } from '@mui/material/Button'; export function Widget() { return <Button>甲</Button>; }")?;
        scratch.write("src/style.css", "body { color: red; }")?;
        let project = Project::load(&scratch.0)?;
        let mut session = AnalysisSession::new();
        let first = project.analyze(&mut session)?;
        assert!(
            !first.program.has_errors(),
            "{:?}",
            first.program.diagnostics
        );
        assert_eq!(first.program.modules.len(), 2);
        let noop = project.analyze(&mut session)?;
        assert!(noop.stats.parsed.is_empty());
        assert_eq!(noop.stats.reused.len(), 2);
        scratch.write("src/style.css", "body { color: blue; }")?;
        let css = project.analyze(&mut session)?;
        assert_eq!(css.program, noop.program);
        assert!(css.stats.parsed.is_empty());
        scratch.write("src/Widget.tsx", "import './style.css'; import { Button } from '@mui/material/Button'; export function Widget() { return <Button>乙</Button>; }")?;
        let changed = project.analyze(&mut session)?;
        assert_eq!(changed.stats.parsed, vec![widget]);
        assert_eq!(changed.stats.reused.len(), 1);
        Ok(())
    }

    #[test]
    fn relative_external_is_validated_per_importer_and_removed_stylesheets_fail()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let scratch = Scratch::new()?;
        scratch.write("kenvoy.toml", "")?;
        scratch.write(
            "src/app.tsx",
            "import './a/Widget'; import './b/Widget'; export default <root />;",
        )?;
        scratch.write(
            "src/a/Widget.tsx",
            "import './style.css'; export const a = 1;",
        )?;
        scratch.write(
            "src/b/Widget.tsx",
            "import './style.css'; export const b = 2;",
        )?;
        scratch.write("src/a/style.css", "a {}")?;
        let project = Project::load(&scratch.0)?;
        let mut session = AnalysisSession::new();
        let error = project
            .analyze(&mut session)
            .expect_err("另一个目录不存在的同名CSS不能被external放行");
        assert!(
            matches!(error, Error::Dsl(ref diagnostics) if diagnostics.iter().any(|diagnostic| diagnostic.code == "KV3006" && diagnostic.source.path.ends_with("b/Widget.tsx")))
        );
        let b = scratch.write("src/b/style.css", "b {}")?;
        let valid = project.analyze(&mut session)?;
        assert!(!valid.program.has_errors());
        fs::remove_file(b)?;
        assert!(project.analyze(&mut session).is_err());
        Ok(())
    }

    #[test]
    fn reserved_paths_and_nonexistent_case_aliases_cannot_overlap_data_or_sources()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let scratch = Scratch::new()?;
        let root = scratch.0.canonicalize()?;
        for name in [
            ".git",
            "safe/.git/objects",
            "node_modules/output",
            ".kenvoy/cache",
            "data/file.sqlite:secret",
            "data/NUL.sqlite",
            "data/CON",
            "data/trailing.",
            "data/trailing ",
        ] {
            assert!(safe_path(&root, Path::new(name)).is_err(), "{name}");
        }
        assert!(safe_path(&root, Path::new("space name/data.sqlite")).is_ok());
        scratch.write("kenvoy.toml", "[build]\nout_dir='data/dist'")?;
        assert!(Project::load(&scratch.0).is_err());
        #[cfg(windows)]
        {
            scratch.write("kenvoy.toml", "[build]\nout_dir='DATA/dist'")?;
            assert!(Project::load(&scratch.0).is_err());
        }
        Ok(())
    }

    #[test]
    fn package_allowlist_cannot_encode_relative_or_absolute_module_escape()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let scratch = Scratch::new()?;
        scratch.write("kenvoy.toml", "")?;
        let project = Project::load(&scratch.0)?;
        for package in [
            "./local",
            "../outside",
            "C:\\private",
            "@scope/name/deep",
            "node:fs",
        ] {
            scratch.write(
                "package.json",
                &serde_json::json!({"dependencies":{package:"1.0.0"}}).to_string(),
            )?;
            assert!(project.external_packages().is_err(), "{package}");
        }
        scratch.write(
            "package.json",
            r#"{"dependencies":{"@scope/name":"1.0.0","react-library":"2.0.0"}}"#,
        )?;
        assert!(project.external_packages()?.contains("@scope/name"));
        Ok(())
    }

    #[test]
    fn create_preserves_existing_empty_and_nonempty_targets()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let scratch = Scratch::new()?;
        let target = scratch.0.join("app");
        fs::create_dir(&target)?;
        assert!(create(&target).is_err());
        assert_eq!(fs::read_dir(&target)?.count(), 0);
        fs::write(target.join("user.txt"), "用户内容")?;
        assert!(create(&target).is_err());
        assert_eq!(fs::read_to_string(target.join("user.txt"))?, "用户内容");
        let fresh = scratch.0.join("fresh");
        create(&fresh)?;
        assert!(fresh.join("kenvoy.toml").is_file());
        assert_eq!(fs::read_to_string(fresh.join("src/app.tsx"))?, CUSTOMER_APP);
        Ok(())
    }
}
