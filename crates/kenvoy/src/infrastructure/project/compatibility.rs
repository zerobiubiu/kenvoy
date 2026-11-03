use std::cmp::Ordering;

use semver::Version;

use super::{
    Project,
    config::ProjectConfig,
    error::{ProjectError as Error, Result},
};

pub(super) const CURRENT_PROJECT_FORMAT: &str = "0.3.0";
const SUPPORTED_FORMATS: &str = ">=0.2.0, <0.4.0";
const FORMAT_MIN: Version = Version::new(0, 2, 0);
const FORMAT_MAX: Version = Version::new(0, 4, 0);

impl Project {
    /// 加载配置不执行兼容性门禁，历史查询可以读取尚未升级的项目。
    pub fn check_compatibility(&self) -> Result<()> {
        if self.config.i18n.is_some() {
            check_i18n(&self.config.project)?;
        }
        check(&self.config.project, env!("CARGO_PKG_VERSION"))
    }
}

pub(super) fn check_i18n(config: &ProjectConfig) -> Result<()> {
    let minimum = Version::new(0, 3, 0);
    if [
        config.format_version.as_deref(),
        config.min_cli_version.as_deref(),
    ]
    .into_iter()
    .any(|raw| {
        raw.and_then(|raw| Version::parse(raw).ok())
            .is_none_or(|version| version.cmp_precedence(&minimum) == Ordering::Less)
    }) {
        return Err(Error::InvalidConfig(
            "启用 i18n 要求 project.format_version >=0.3.0 且 project.min_cli_version >=0.3.0；旧配置不会自动升级，请明确确认兼容性后填写版本字段".into(),
        ));
    }
    Ok(())
}

fn parse(field: &str, raw: Option<&str>, problems: &mut Vec<String>) -> Option<Version> {
    let Some(raw) = raw else {
        problems.push(format!("缺少 {field}；旧配置不会自动推定此字段"));
        return None;
    };
    match Version::parse(raw) {
        Ok(version) => Some(version),
        Err(error) => {
            problems.push(format!(
                "{field} 必须是完整 SemVer（如 1.2.3），收到 {raw:?}：{error}"
            ));
            None
        }
    }
}

pub(super) fn check(config: &ProjectConfig, actual_cli: &str) -> Result<()> {
    let mut problems = Vec::new();
    let cli = parse("实际 Kenvoy CLI 版本", Some(actual_cli), &mut problems);
    parse("project.version", Some(&config.version), &mut problems);
    let minimum = parse(
        "project.min_cli_version",
        config.min_cli_version.as_deref(),
        &mut problems,
    );
    let format = parse(
        "project.format_version",
        config.format_version.as_deref(),
        &mut problems,
    );
    if let (Some(cli), Some(minimum)) = (&cli, &minimum) {
        if cli.pre.is_empty() && !minimum.pre.is_empty() {
            problems.push("稳定 Kenvoy CLI 不接受预发布的 project.min_cli_version".into());
        }
        if cli.cmp_precedence(minimum) == Ordering::Less {
            problems.push(format!(
                "实际 Kenvoy CLI {actual_cli} 低于项目要求的最低版本 {minimum}"
            ));
        }
    }
    if let Some(format) = format {
        if cli.as_ref().is_some_and(|cli| cli.pre.is_empty()) && !format.pre.is_empty() {
            problems.push("稳定 Kenvoy CLI 不接受预发布的 project.format_version".into());
        }
        if format.cmp_precedence(&FORMAT_MIN) == Ordering::Less
            || format.cmp_precedence(&FORMAT_MAX) != Ordering::Less
        {
            problems.push(format!(
                "项目格式 {format} 不在 CLI 支持范围 {SUPPORTED_FORMATS} 内"
            ));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(Error::InvalidConfig(format!(
        "项目兼容性检查失败\n实际 Kenvoy CLI：{actual_cli}\n项目版本：{}\n最低 CLI：{}\n项目格式：{}\n支持的项目格式范围：{SUPPORTED_FORMATS}；当前创建格式：{CURRENT_PROJECT_FORMAT}\n- {}\n旧配置升级路径：先用原兼容 CLI 核对并备份源码和数据库，完成配置及正式迁移链升级，再显式填写 project.min_cli_version 与 project.format_version；旧数据库先执行 kenvoy migrate baseline 预览并按校验值确认，不能仅改版本字段绕过升级。kenvoy migrate history 仍可查看历史。",
        config.version,
        config.min_cli_version.as_deref().unwrap_or("<缺失>"),
        config.format_version.as_deref().unwrap_or("<缺失>"),
        problems.join("\n- "),
    )))
}
