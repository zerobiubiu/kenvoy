//! Rust 拥有测试项目和进程；JavaScript 仅驱动真实 Playwright 浏览器及页面断言。

use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::{Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use kenvoy_tests::{Fixture, ProcessGuard, Server, TestResult};
use serde::Deserialize;
use serde_json::{Value, json};

const PREFIX: &str = "KENVOY_FIXTURE_RPC:";
const MAX_LINE: usize = 1024 * 1024;
static BROWSER_LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
struct Request {
    id: u64,
    #[serde(flatten)]
    operation: Operation,
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Fixture,
    Command {
        args: Vec<String>,
        input: String,
        timeout: u64,
    },
    Start {
        mode: String,
        args: Vec<String>,
        timeout: u64,
        env: BTreeMap<String, String>,
    },
    Stop {
        server: u64,
    },
    Logs {
        server: u64,
    },
    Close,
}

#[derive(Default)]
struct State {
    fixture: Option<Fixture>,
    servers: BTreeMap<u64, Server>,
    next_server: u64,
    closed: bool,
    completed: bool,
}

impl State {
    fn fixture(&self) -> TestResult<&Fixture> {
        self.fixture
            .as_ref()
            .ok_or_else(|| "浏览器尚未创建 Rust 夹具".into())
    }

    fn execute(&mut self, operation: Operation) -> TestResult<Value> {
        if self.closed {
            return Err("浏览器夹具已关闭".into());
        }
        match operation {
            Operation::Fixture => {
                if self.fixture.is_some() {
                    return Err("一个浏览器流程只能拥有一个项目夹具".into());
                }
                let fixture = Fixture::new("browser")?;
                fixture.create()?;
                let paths = json!({"root":fixture.root(),"project":fixture.project()});
                self.fixture = Some(fixture);
                Ok(paths)
            }
            Operation::Command {
                args,
                input,
                timeout,
            } => {
                let args: Vec<_> = args.iter().map(String::as_str).collect();
                let output =
                    self.fixture()?
                        .command_with(&args, &input, timeout_duration(timeout)?)?;
                if !output.status.success() {
                    return Err(format!(
                        "CLI 退出 {}：{}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    )
                    .into());
                }
                Ok(Value::String(String::from_utf8(output.stdout)?))
            }
            Operation::Start {
                mode,
                args,
                timeout,
                env,
            } => {
                if !matches!(mode.as_str(), "dev" | "run") {
                    return Err("浏览器服务模式只能为 dev 或 run".into());
                }
                let args: Vec<_> = args.iter().map(String::as_str).collect();
                let server =
                    self.fixture()?
                        .start_with(&mode, &args, timeout_duration(timeout)?, &env)?;
                self.next_server += 1;
                let result = json!({"id":self.next_server,"origin":server.origin()});
                self.servers.insert(self.next_server, server);
                Ok(result)
            }
            Operation::Stop { server } => {
                self.servers
                    .get_mut(&server)
                    .ok_or("服务编号不存在")?
                    .stop()?;
                Ok(Value::Null)
            }
            Operation::Logs { server } => Ok(Value::String(
                self.servers.get(&server).ok_or("服务编号不存在")?.logs()?,
            )),
            Operation::Close => {
                self.close()?;
                self.completed = true;
                Ok(Value::Null)
            }
        }
    }

    fn close(&mut self) -> TestResult {
        let mut failures = Vec::new();
        for server in self.servers.values_mut() {
            if let Err(error) = server.stop() {
                failures.push(error.to_string());
            }
        }
        if !failures.is_empty() {
            return Err(format!("浏览器夹具服务回收失败：{}", failures.join("；")).into());
        }
        self.servers.clear();
        self.fixture = None;
        self.closed = true;
        Ok(())
    }
}

fn timeout_duration(milliseconds: u64) -> TestResult<Duration> {
    if !(1..=240_000).contains(&milliseconds) {
        return Err("夹具请求超时必须在 1 至 240000 毫秒之间".into());
    }
    Ok(Duration::from_millis(milliseconds))
}

enum Event {
    Line { stderr: bool, line: String },
    End,
    Failure(String),
}

struct ReplyWriter {
    requests: Option<mpsc::SyncSender<Vec<u8>>>,
    acknowledgements: mpsc::Receiver<io::Result<()>>,
    finished: mpsc::Receiver<()>,
    thread: JoinHandle<()>,
}

impl ReplyWriter {
    fn new(mut input: impl Write + Send + 'static) -> Self {
        let (requests, receiver) = mpsc::sync_channel::<Vec<u8>>(1);
        let (acknowledgements, replies) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            while let Ok(bytes) = receiver.recv() {
                let result = input.write_all(&bytes).and_then(|()| input.flush());
                let failed = result.is_err();
                if acknowledgements.send(result).is_err() || failed {
                    break;
                }
            }
            let _ = finished.send(());
        });
        Self {
            requests: Some(requests),
            acknowledgements: replies,
            finished: done,
            thread,
        }
    }

    fn send(&self, bytes: Vec<u8>, timeout: Duration) -> TestResult {
        self.requests
            .as_ref()
            .ok_or("浏览器响应通道已关闭")?
            .try_send(bytes)
            .map_err(|_| "浏览器响应通道繁忙或已关闭")?;
        self.acknowledgements
            .recv_timeout(timeout)
            .map_err(|_| "浏览器响应写入超过期限或写入线程提前退出")??;
        Ok(())
    }

    fn finish(mut self) -> TestResult {
        self.requests = None;
        self.finished
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "自有进程终止后浏览器响应线程未及时退出")?;
        self.thread.join().map_err(|_| "浏览器响应写入线程异常")?;
        Ok(())
    }
}

fn read_pipe(
    pipe: impl Read + Send + 'static,
    stderr: bool,
    sender: mpsc::SyncSender<Event>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let mut bytes = Vec::new();
            match reader
                .by_ref()
                .take((MAX_LINE + 1) as u64)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) => break,
                Ok(_) if bytes.len() > MAX_LINE => {
                    let _ = sender.send(Event::Failure("浏览器输出行超过 1 MiB 限制".into()));
                    break;
                }
                Ok(_) => {
                    let line = String::from_utf8_lossy(&bytes)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    if sender.send(Event::Line { stderr, line }).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(Event::Failure(format!("读取浏览器输出失败：{error}")));
                    break;
                }
            }
        }
        let _ = sender.send(Event::End);
    })
}

fn dispatch_line(
    line: &str,
    state: &mut State,
    writer: &ReplyWriter,
    deadline: Instant,
) -> TestResult {
    let Some(payload) = line.strip_prefix(PREFIX) else {
        println!("{line}");
        return Ok(());
    };
    let request: Request = serde_json::from_str(payload)
        .map_err(|error| format!("浏览器夹具请求格式无效：{error}"))?;
    let response = match state.execute(request.operation) {
        Ok(result) => json!({"id":request.id,"result":result}),
        Err(error) => json!({"id":request.id,"error":error.to_string()}),
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    writer.send(
        bytes,
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(10)),
    )
}

fn bridge(
    process: &mut ProcessGuard,
    state: &mut State,
    timeout: Duration,
) -> TestResult<ExitStatus> {
    let input = process
        .child_mut()
        .stdin
        .take()
        .ok_or("浏览器 stdin 未连接")?;
    let output = process
        .child_mut()
        .stdout
        .take()
        .ok_or("浏览器 stdout 未连接")?;
    let errors = process
        .child_mut()
        .stderr
        .take()
        .ok_or("浏览器 stderr 未连接")?;
    let (sender, receiver) = mpsc::sync_channel(64);
    let readers = [
        read_pipe(output, false, sender.clone()),
        read_pipe(errors, true, sender),
    ];
    let writer = ReplyWriter::new(input);
    let deadline = Instant::now() + timeout;
    let mut ended = 0;
    let result: TestResult<ExitStatus> = (|| loop {
        if Instant::now() >= deadline {
            return Err(format!("浏览器流程超过 {} 秒限制", timeout.as_secs()).into());
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Line { stderr: true, line }) => eprintln!("{line}"),
            Ok(Event::Line {
                stderr: false,
                line,
            }) => dispatch_line(&line, state, &writer, deadline)?,
            Ok(Event::Failure(error)) => return Err(error.into()),
            Ok(Event::End) => ended += 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
        if let Some(status) = process.try_wait()? {
            if ended == 2 {
                if status.success() && (!state.completed || state.next_server == 0) {
                    return Err("浏览器在完成服务验收与关闭协议前提前退出".into());
                }
                return Ok(status);
            }
            if !status.success() {
                return Err(format!("浏览器进程提前退出：{status}").into());
            }
        }
    })();
    // 先停止持有管道读端的自有进程树，解除可能阻塞的 write_all，再回收写入线程。
    let stopped = process.stop(Duration::from_secs(10));
    let writer_finished = writer.finish();
    let cleaned = state.close();
    // 失败也要等待管道 EOF；子孙进程继承句柄时，此检查能发现残留。
    let drain_deadline = Instant::now() + Duration::from_secs(5);
    while ended < 2 && Instant::now() < drain_deadline {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::End) => ended += 1,
            Ok(Event::Line { stderr: true, line }) => eprintln!("{line}"),
            Ok(Event::Line {
                stderr: false,
                line,
            }) if !line.starts_with(PREFIX) => println!("{line}"),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if ended != 2 {
        return Err("浏览器进程回收后管道仍未关闭，可能存在自有后代进程残留".into());
    }
    for reader in readers {
        reader.join().map_err(|_| "浏览器输出读取线程异常")?;
    }
    stopped?;
    writer_finished?;
    cleaned?;
    result
}

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
