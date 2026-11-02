//! 只对操作系统、架构均匹配的产物做限时 --version 核验。

use super::{options::Options, temporary::TemporaryDirectory};
use crate::Result;
use std::{
    fs::File,
    io::Read,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn verify(options: &Options) -> Result<()> {
    if !options.target.host_compatible() {
        println!(
            "跳过 --version：{} 与当前 {}-{} host 不同；仅校验格式和架构",
            options.target.triple,
            std::env::consts::ARCH,
            std::env::consts::OS,
        );
        return Ok(());
    }
    let temporary = TemporaryDirectory::create()?;
    let output = temporary.path().join("version.stdout");
    let mut command = Command::new(&options.binary);
    command
        .arg("--version")
        .current_dir(temporary.path())
        .stdin(Stdio::null())
        .stdout(File::create_new(&output)?)
        .stderr(Stdio::inherit());
    super::super::process::hide_window(&mut command);
    let mut child = RunningChild(command.spawn()?);
    drop(command);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(15) {
            return Err("kenvoy --version 超过 15 秒，已终止本次子进程".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    if !status.success() {
        return Err(format!("kenvoy --version 退出：{status}").into());
    }
    let mut text = String::new();
    File::open(&output)?.take(4097).read_to_string(&mut text)?;
    if text.len() > 4096 || text.trim() != format!("kenvoy {}", options.version) {
        return Err(format!(
            "kenvoy --version 与分发版本 {} 不匹配：{text:?}",
            options.version
        )
        .into());
    }
    println!("host 兼容产物的限时版本核验：{}", text.trim());
    Ok(())
}

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            if let Err(error) = self.0.kill() {
                eprintln!("无法终止本次 --version 子进程：{error}");
            }
            if let Err(error) = self.0.wait() {
                eprintln!("无法回收本次 --version 子进程：{error}");
            }
        }
    }
}
