//! 有界管道、RPC 响应与关闭期限。

use super::protocol::{PREFIX, Request, State};
use kenvoy_tests::{ProcessGuard, TestResult};
use serde_json::json;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::ExitStatus,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_LINE: usize = 1024 * 1024;

enum Event {
    Line { stderr: bool, line: String },
    End,
    Failure(String),
}

pub(super) struct ReplyWriter {
    requests: Option<mpsc::SyncSender<Vec<u8>>>,
    acknowledgements: mpsc::Receiver<io::Result<()>>,
    finished: mpsc::Receiver<()>,
    thread: JoinHandle<()>,
}

impl ReplyWriter {
    pub(super) fn new(mut input: impl Write + Send + 'static) -> Self {
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

    pub(super) fn send(&self, bytes: Vec<u8>, timeout: Duration) -> TestResult {
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

    pub(super) fn finish(mut self) -> TestResult {
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

pub(super) fn bridge(
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
