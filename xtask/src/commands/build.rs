//! 默认前端与真实 Kenvoy 可执行文件构建。

use super::process::{cargo_command, hide_window, run};
use crate::Result;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn build_frontend(root: &Path) -> Result<()> {
    // npm.cmd 是 Windows 的批处理入口，仅运行仓库已声明的 build，不执行安装。
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let mut command = Command::new(npm);
    command
        .current_dir(root.join("crates/kenvoy/frontend"))
        .args(["run", "build"]);
    hide_window(&mut command);
    run(&mut command, "默认前端构建")
}

pub(super) fn build_host(root: &Path, arguments: &[OsString]) -> Result<PathBuf> {
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
    use std::env;

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
}
