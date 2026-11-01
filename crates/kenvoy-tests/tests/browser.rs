//! Rust 拥有测试项目和进程；JavaScript 仅驱动真实 Playwright 浏览器及页面断言。

#[path = "browser_support/protocol.rs"]
mod protocol;
#[path = "browser_support/transport.rs"]
mod transport;

use kenvoy_tests::{ProcessGuard, TestResult};
use protocol::{PREFIX, State};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};
use transport::{ReplyWriter, bridge};

static BROWSER_LOCK: Mutex<()> = Mutex::new(());

fn browser_case(name: &str) -> TestResult {
    // npm 缓存准备与浏览器 HMR 流程串行执行，避免并行启动造成资源争用。
    let _serial = BROWSER_LOCK.lock().map_err(|_| "浏览器测试串行锁损坏")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let frontend = repository.join("crates/kenvoy/frontend");
    if !frontend
        .join("node_modules/@playwright/test/package.json")
        .is_file()
    {
        return Err("缺少已安装的 Playwright 依赖；请先按测试文档准备前端依赖和 Chromium".into());
    }
    let mut command = Command::new("node");
    command
        .arg(frontend.join("scripts").join(format!("{name}-e2e.mjs")))
        .current_dir(&frontend)
        .env("KENVOY_TEST_RPC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = ProcessGuard::spawn(&mut command)?;
    let mut state = State::default();
    let status = bridge(&mut process, &mut state, Duration::from_secs(720))?;
    if !status.success() {
        return Err(format!("{name} 浏览器验收失败：{status}").into());
    }
    Ok(())
}

macro_rules! browser_tests {
    ($($name:ident => $script:literal),+ $(,)?) => { $(
        #[test]
        #[ignore = "需已构建 Kenvoy、Node、Playwright 和 Chromium；显式运行真实浏览器验收"]
        fn $name() -> TestResult { browser_case($script) }
    )+ };
}

browser_tests! {
    forms => "forms",
    fields => "fields",
    admin => "admin",
    extensions => "extensions",
    options => "options",
    production => "production",
    watcher => "watcher",
}

#[test]
fn malformed_protocol_stops_owned_process_tree_without_browser() -> TestResult {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "protocol_child_helper", "--nocapture"])
        .env("KENVOY_PROTOCOL_HELPER", "parent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = ProcessGuard::spawn(&mut command)?;
    let mut state = State::default();
    let error = bridge(&mut process, &mut state, Duration::from_secs(10))
        .expect_err("非法协议必须失败并回收自有进程树");
    assert!(error.to_string().contains("请求格式无效"), "{error}");
    assert!(process.try_wait()?.is_some());
    assert!(state.closed && state.fixture.is_none() && state.servers.is_empty());
    Ok(())
}

#[test]
fn blocked_protocol_response_has_deadline_and_releases_writer_without_browser() -> TestResult {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "protocol_child_helper", "--nocapture"])
        .env("KENVOY_PROTOCOL_HELPER", "unread_stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut process = ProcessGuard::spawn(&mut command)?;
    let input = process
        .child_mut()
        .stdin
        .take()
        .ok_or("故障注入 stdin 缺失")?;
    let writer = ReplyWriter::new(input);
    let error = writer
        .send(vec![b'x'; 8 * 1024 * 1024], Duration::from_millis(100))
        .expect_err("子进程不读取响应时必须按期返回");
    let stopped = process.stop(Duration::from_secs(5));
    let finished = writer.finish();
    assert!(error.to_string().contains("响应写入超过期限"), "{error}");
    stopped?;
    finished?;
    assert!(process.try_wait()?.is_some());
    Ok(())
}

#[test]
fn premature_protocol_exit_is_not_a_success_without_browser() -> TestResult {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "protocol_child_helper", "--nocapture"])
        .env("KENVOY_PROTOCOL_HELPER", "early_exit")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = ProcessGuard::spawn(&mut command)?;
    let mut state = State::default();
    let error = bridge(&mut process, &mut state, Duration::from_secs(10))
        .expect_err("提前退出不能冒充浏览器验收通过");
    assert!(error.to_string().contains("关闭协议前提前退出"), "{error}");
    assert!(process.try_wait()?.is_some());
    assert!(state.closed && !state.completed);
    Ok(())
}

#[test]
fn protocol_child_helper() -> TestResult {
    match std::env::var("KENVOY_PROTOCOL_HELPER").as_deref() {
        Ok("unread_stdin") => {}
        Ok("descendant") => {
            println!("DESCENDANT_READY");
            io::stdout().flush()?;
        }
        Ok("parent") => {
            let mut child = Command::new(std::env::current_exe()?)
                .args(["--exact", "protocol_child_helper", "--nocapture"])
                .env("KENVOY_PROTOCOL_HELPER", "descendant")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?;
            let output = child.stdout.take().ok_or("故障注入子进程 stdout 缺失")?;
            for line in BufReader::new(output).lines() {
                if line? == "DESCENDANT_READY" {
                    println!("{PREFIX}{{invalid-json");
                    io::stdout().flush()?;
                    break;
                }
            }
            // 子进程故意保持运行，由测试拥有的外层进程树守卫负责失败清理。
            child.wait()?;
        }
        _ => return Ok(()),
    }
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
