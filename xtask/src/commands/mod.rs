//! 构建、分发、测试与仓库检查的命令编排。

mod build;
mod distribution;
mod options;
mod process;
mod version_tags;

use crate::{
    Result,
    checks::{architecture, docs},
};
use build::{build_frontend, build_host};
use options::{CargoOptions, test_arguments};
pub(crate) use process::cargo_command;
use process::run;
use std::{env, ffi::OsString, path::Path};

pub(crate) fn execute() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("无法定位 Workspace 根目录")?;
    let mut arguments = env::args_os().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".into());
    let tail: Vec<OsString> = arguments.collect();
    match command.to_str() {
        Some("help" | "--help" | "-h") => {
            println!(
                "Kenvoy 工程任务\n\
                 cargo xtask check          检查架构与文档\n\
                 cargo xtask architecture   检查 Cargo 与源码分层边界\n\
                 cargo xtask docs           检查文档编码、链接和索引\n\
                 cargo xtask build          构建默认前端与 Kenvoy\n\
                 cargo xtask test           构建并执行 Workspace Rust 测试\n\
                 cargo xtask browser        构建并执行显式浏览器测试\n\
                 cargo xtask dist           校验既有二进制并生成 ZIP 与 SHA256\n\
                 cargo xtask version-tags   只读预览本地版本标签；--apply 创建缺失标签\n\
                 build/test/browser 支持 --release、--profile、--target-dir、--target；\n\
                 test 的其他参数传给 cargo test，browser 的其他参数传给浏览器测试。\n\
                 dist 必填 --binary、--target、--version、--output；可选 --readme、--license、--archive-name NAME.zip。\n\
                 dist 只对 host 兼容产物限时执行 --version，不执行功能测试。\n\
                 --archive-name 只接受普通 ZIP basename；version-tags 支持 --repo PATH 选择隔离仓库。\n\
                 build/test/browser 的 Cargo 子命令固定使用 --offline --locked；不会安装依赖。"
            );
            Ok(())
        }
        Some("check" | "architecture" | "docs") => {
            if !tail.is_empty() {
                return Err("该检查不接受额外参数".into());
            }
            if command != "docs" {
                architecture::verify(root)?;
            }
            if command != "architecture" {
                docs::verify(root)?;
            }
            Ok(())
        }
        Some("dist") => distribution::execute(root, &tail),
        Some("version-tags") => version_tags::execute(root, &tail),
        Some("build" | "test" | "browser") => {
            let options = CargoOptions::parse(&tail)?;
            if command == "build" && !options.test_arguments.is_empty() {
                return Err("build 只接受构建位置、目标和 profile 参数".into());
            }
            build_frontend(root)?;
            let binary = build_host(root, &options.build_arguments)?;
            if command == "build" {
                println!("Kenvoy 构建完成：{}", binary.display());
                return Ok(());
            }
            let mut cargo = cargo_command(root);
            cargo.args(["test", "--offline", "--locked"]);
            if command == "browser" {
                cargo.args(["-p", "kenvoy-tests", "--test", "browser"]);
            } else {
                cargo.arg("--workspace");
            }
            cargo.args(&options.build_arguments);
            cargo.args(test_arguments(
                &options.test_arguments,
                command == "browser",
            ));
            cargo.env("KENVOY_BINARY", &binary);
            run(&mut cargo, "Rust 测试")
        }
        _ => Err(format!(
            "未知工程任务：{}；请运行 cargo xtask help",
            command.to_string_lossy()
        )
        .into()),
    }
}
