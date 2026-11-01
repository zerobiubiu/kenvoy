//! 工程子进程的统一启动与退出码处理。

use crate::Result;
use std::{env, path::Path, process::Command};

pub(crate) fn cargo_command(root: &Path) -> Command {
    let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root);
    hide_window(&mut command);
    command
}

pub(super) fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub(super) fn run(command: &mut Command, label: &str) -> Result<()> {
    let status = command
        .status()
        .map_err(|error| format!("无法启动{label}：{error}"))?;
    if !status.success() {
        return Err(format!("{label}退出：{status}").into());
    }
    Ok(())
}
