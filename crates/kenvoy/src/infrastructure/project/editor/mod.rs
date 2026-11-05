//! 只生成编辑器声明及缺失的开发配置，不解析应用、不安装依赖、不访问数据库。
mod cache;
mod config;
mod files;

#[cfg(test)]
mod tests;

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use super::{
    Project,
    config::Config,
    error::{ProjectError as Error, Result, io},
};
use crate::infrastructure::frontend::EDITOR_TYPES;

#[derive(Serialize)]
pub struct Configuration {
    pub path: &'static str,
    pub state: &'static str,
    pub message: &'static str,
}

#[derive(Serialize)]
pub struct Prepared {
    pub status: &'static str,
    pub declaration_directory: &'static str,
    pub declarations: cache::Refreshed,
    pub tsconfig: Configuration,
    pub package: Configuration,
    pub configuration_review_required: bool,
    pub dependency_installation_performed: bool,
    pub required_tsconfig: Value,
    pub recommended_package: Value,
    pub install_command: String,
    pub typecheck_command: &'static str,
    pub instructions: Vec<&'static str>,
}

pub fn prepare(project: &Project) -> Result<Prepared> {
    prepare_at(&project.root, &project.config)
}

pub(super) fn prepare_at(root: &Path, project_config: &Config) -> Result<Prepared> {
    let root = root.canonicalize().map_err(|error| io(root, error))?;
    // 即使已有 JSON 损坏，也只检查文件边界，不读取、解析或改写用户配置。
    files::path(&root, "tsconfig.json")?;
    files::path(&root, "package.json")?;
    let required_tsconfig = config::tsconfig(project_config);
    let recommended_package = config::package()?;
    for required in [
        "sdk.d.ts",
        "presentation/jsx-runtime.d.ts",
        "presentation/jsx-dev-runtime.d.ts",
    ] {
        if !EDITOR_TYPES.iter().any(|(name, _)| *name == required) {
            return Err(Error::InvalidConfig(format!(
                "内嵌编辑器声明不完整，缺少 {required}"
            )));
        }
    }
    let directory = files::directory(&root, Path::new(".kenvoy/types"))?;
    let declarations = cache::refresh(&directory, EDITOR_TYPES)?;
    let tsconfig_created = files::create_configuration(
        &root,
        "tsconfig.json",
        &config::encoded(&required_tsconfig)?,
    )?;
    let package_created = files::create_configuration(
        &root,
        "package.json",
        &config::encoded(&recommended_package)?,
    )?;
    let install_command = config::install_command(&recommended_package, package_created)?;
    let mut instructions = Vec::new();
    if !tsconfig_created {
        instructions.push("已保留 tsconfig.json 原始字节，未确认其配置有效；请手工对照 required_tsconfig，接入三项 paths、jsxImportSource 与 JSX/strict 编译选项，保留自己的其他设置。");
    }
    if !package_created {
        instructions.push("已保留 package.json 原始字节，未确认其配置有效；请手工接入 recommended_package 的 devDependencies 与 scripts.typecheck，不要覆盖自己的其他依赖、脚本或配置。损坏的 JSON 需先手工修复。");
    }
    instructions.push("声明已生成不代表编辑器配置和依赖已就绪。请在项目根目录显式执行 install_command，再执行 typecheck_command；本命令没有运行 npm，也没有编译或执行项目源码。");
    instructions.push("React 运行时与 @kenvoy/client 仍由 Kenvoy 共享；不要为此安装 react、react-dom 或空壳 @kenvoy/client。CLI 升级后可再次运行 kenvoy types 刷新声明。");
    Ok(Prepared {
        status: "declarations_generated",
        declaration_directory: ".kenvoy/types",
        declarations,
        tsconfig: Configuration {
            path: "tsconfig.json",
            state: if tsconfig_created {
                "created"
            } else {
                "preserved"
            },
            message: if tsconfig_created {
                "已创建默认编辑器配置"
            } else {
                "保留原文件，需手工核对接线"
            },
        },
        package: Configuration {
            path: "package.json",
            state: if package_created {
                "created"
            } else {
                "preserved"
            },
            message: if package_created {
                "已创建私有开发类型依赖清单"
            } else {
                "保留原文件，需手工核对开发依赖与脚本"
            },
        },
        configuration_review_required: !tsconfig_created || !package_created,
        dependency_installation_performed: false,
        required_tsconfig,
        recommended_package,
        install_command,
        typecheck_command: "npm run typecheck",
        instructions,
    })
}
