//! 仓库工程任务；不安装依赖，不参与应用运行。

mod architecture;
mod docs;

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = execute() {
        eprintln!("工程任务失败：{error}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
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
                 cargo xtask architecture   检查实际 Cargo 依赖边界\n\
                 cargo xtask docs           检查文档编码、链接和索引\n\
                 cargo xtask build          构建默认前端与 Kenvoy\n\
                 cargo xtask test           构建并执行 Workspace Rust 测试\n\
                 cargo xtask browser        构建并执行显式浏览器测试\n\
                 build/test/browser 支持 --release、--profile、--target-dir、--target；\n\
                 test 的其他参数传给 cargo test，browser 的其他参数传给浏览器测试。\n\
                 Cargo 子命令固定使用 --offline --locked；不会安装依赖。"
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

fn test_arguments(arguments: &[OsString], ignored: bool) -> Vec<OsString> {
    let mut arguments = arguments.to_vec();
    if ignored {
        if let Some(separator) = arguments.iter().position(|item| item == "--") {
            if !arguments[separator + 1..]
                .iter()
                .any(|item| item == "--ignored")
            {
                arguments.insert(separator + 1, "--ignored".into());
            }
        } else {
            arguments.extend([OsString::from("--"), OsString::from("--ignored")]);
        }
    }
    arguments
}

#[derive(Default)]
struct CargoOptions {
    build_arguments: Vec<OsString>,
    test_arguments: Vec<OsString>,
}

impl CargoOptions {
    fn parse(arguments: &[OsString]) -> Result<Self> {
        let mut result = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            let argument = &arguments[index];
            match argument.to_str() {
                Some("--offline" | "--locked") => {}
                Some("--release") => result.build_arguments.push(argument.clone()),
                Some("--profile" | "--target-dir" | "--target") => {
                    result.build_arguments.push(argument.clone());
                    index += 1;
                    result
                        .build_arguments
                        .push(arguments.get(index).ok_or("构建选项缺少参数")?.clone());
                }
                Some(value)
                    if ["--profile=", "--target-dir=", "--target="]
                        .iter()
                        .any(|prefix| value.starts_with(prefix)) =>
                {
                    result.build_arguments.push(argument.clone());
                }
                Some("--") => {
                    result.test_arguments.extend_from_slice(&arguments[index..]);
                    break;
                }
                _ => result.test_arguments.push(argument.clone()),
            }
            index += 1;
        }
        Ok(result)
    }
}

pub(crate) fn cargo_command(root: &Path) -> Command {
    let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root);
    hide_window(&mut command);
    command
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn run(command: &mut Command, label: &str) -> Result<()> {
    let status = command
        .status()
        .map_err(|error| format!("无法启动{label}：{error}"))?;
    if !status.success() {
        return Err(format!("{label}退出：{status}").into());
    }
    Ok(())
}

fn build_frontend(root: &Path) -> Result<()> {
    // npm.cmd 是 Windows 的批处理入口，仅运行仓库已声明的 build，不执行安装。
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let mut command = Command::new(npm);
    command
        .current_dir(root.join("crates/kenvoy/frontend"))
        .args(["run", "build"]);
    hide_window(&mut command);
    run(&mut command, "默认前端构建")
}

fn build_host(root: &Path, arguments: &[OsString]) -> Result<PathBuf> {
    let output = cargo_command(root)
        .args([
            "build",
            "-p",
            "kenvoy",
            "--offline",
            "--locked",
            "--message-format=json-render-diagnostics",
        ])
        .args(arguments)
        .stderr(Stdio::inherit())
        .output()?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(message) = serde_json::from_str::<serde_json::Value>(line)
            && message["reason"] == "compiler-message"
            && let Some(rendered) = message["message"]["rendered"].as_str()
        {
            eprint!("{rendered}");
        }
    }
    if !output.status.success() {
        return Err(format!("Kenvoy 构建退出：{}", output.status).into());
    }
    executable_from_messages(&output.stdout)
}

fn executable_from_messages(bytes: &[u8]) -> Result<PathBuf> {
    let text = std::str::from_utf8(bytes)?;
    let mut executable = None;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let message: serde_json::Value = serde_json::from_str(line)?;
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "kenvoy"
            && message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
            && let Some(path) = message["executable"].as_str()
        {
            executable = Some(PathBuf::from(path));
        }
    }
    let path = executable.ok_or("Cargo 未报告 Kenvoy 可执行文件")?;
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("Cargo 报告的可执行文件无效：{}", path.display()).into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_options_preserve_build_location_and_test_arguments() -> Result<()> {
        let options = CargoOptions::parse(
            &[
                "--target-dir",
                "alternate target",
                "--release",
                "--offline",
                "--test",
                "http",
                "--",
                "--nocapture",
            ]
            .map(OsString::from),
        )?;
        assert_eq!(
            options.build_arguments,
            ["--target-dir", "alternate target", "--release"].map(OsString::from)
        );
        assert_eq!(
            options.test_arguments,
            ["--test", "http", "--", "--nocapture"].map(OsString::from)
        );
        assert!(CargoOptions::parse(&["--target-dir".into()]).is_err());
        Ok(())
    }

    #[test]
    fn missing_executable_is_an_error() {
        assert!(
            executable_from_messages(br#"{"reason":"build-finished","success":true}"#).is_err()
        );
    }

    #[test]
    fn executable_comes_from_cargo_artifact_instead_of_assumed_target_path() -> Result<()> {
        let executable = env::current_exe()?;
        let messages = [
            serde_json::json!({"reason":"compiler-artifact","target":{"name":"helper","kind":["bin"]},"executable":"unrelated"}),
            serde_json::json!({"reason":"compiler-artifact","target":{"name":"kenvoy","kind":["bin"]},"executable":executable}),
            serde_json::json!({"reason":"build-finished","success":true}),
        ].map(|value| value.to_string()).join("\n");
        assert_eq!(executable_from_messages(messages.as_bytes())?, executable);
        Ok(())
    }

    #[test]
    fn browser_ignored_mode_is_kept_with_explicit_test_separator() {
        assert_eq!(
            test_arguments(&["forms".into(), "--".into(), "--nocapture".into()], true),
            ["forms", "--", "--ignored", "--nocapture"].map(OsString::from)
        );
        assert_eq!(
            test_arguments(&[], true),
            ["--", "--ignored"].map(OsString::from)
        );
    }
}
