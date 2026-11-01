//! 浏览器夹具请求协议与服务所有权。

use kenvoy_tests::{Fixture, Server, TestResult};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

pub(super) const PREFIX: &str = "KENVOY_FIXTURE_RPC:";

#[derive(Deserialize)]
pub(super) struct Request {
    pub(super) id: u64,
    #[serde(flatten)]
    pub(super) operation: Operation,
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Operation {
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
pub(super) struct State {
    pub(super) fixture: Option<Fixture>,
    pub(super) servers: BTreeMap<u64, Server>,
    pub(super) next_server: u64,
    pub(super) closed: bool,
    pub(super) completed: bool,
}

impl State {
    fn fixture(&self) -> TestResult<&Fixture> {
        self.fixture
            .as_ref()
            .ok_or_else(|| "浏览器尚未创建 Rust 夹具".into())
    }

    pub(super) fn execute(&mut self, operation: Operation) -> TestResult<Value> {
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

    pub(super) fn close(&mut self) -> TestResult {
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
