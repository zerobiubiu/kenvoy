//! 终止信号只触发一次有序退出；再次收到信号时强制结束，避免关闭阶段永久挂起。
//! Windows 没有 SIGTERM，按控制台关闭/注销事件处理；信号注册失败时退回 Ctrl+C。

use std::{io, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time::sleep};

const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(75);

/// 从启动阶段开始监听终止信号，并保证重复信号或超期关闭能强制结束进程。
pub(super) struct Shutdown {
    requested: watch::Receiver<bool>,
    task: JoinHandle<()>,
}

impl Shutdown {
    pub(super) fn install() -> Self {
        let (sender, requested) = watch::channel(false);
        let mut signals = match Signals::new() {
            Ok(signals) => Some(signals),
            Err(error) => {
                tracing::error!(target:"kenvoy::lifecycle",%error,"终止信号注册失败，退回 Ctrl+C");
                None
            }
        };
        let task = tokio::spawn(async move {
            next_signal(&mut signals).await;
            tracing::info!(
                target:"kenvoy::lifecycle",
                seconds = SHUTDOWN_DEADLINE.as_secs(),
                "收到终止信号，开始优雅退出；再次发送终止信号将强制结束"
            );
            let _ = sender.send(true);
            tokio::select! {
                _ = next_signal(&mut signals) => force_exit(),
                _ = sleep(SHUTDOWN_DEADLINE) => {
                    tracing::warn!(
                        target:"kenvoy::lifecycle",
                        seconds = SHUTDOWN_DEADLINE.as_secs(),
                        "优雅退出超过期限，强制结束；已接受但未完成的写入结果需要核对"
                    );
                    std::process::exit(130)
                }
            }
        });
        Self { requested, task }
    }

    /// 等待首次终止请求；信号任务异常退出时保持等待，不能用错误伪装成退出请求。
    pub(super) fn wait_owned(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut requested = self.requested.clone();
        async move {
            if requested.wait_for(|requested| *requested).await.is_err() {
                tracing::error!(target:"kenvoy::lifecycle","终止信号任务异常退出");
                std::future::pending::<()>().await;
            }
        }
    }

    pub(super) async fn wait(&self) {
        self.wait_owned().await;
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn next_signal(signals: &mut Option<Signals>) {
    match signals {
        Some(signals) => signals.recv().await,
        None => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

fn force_exit() -> ! {
    tracing::warn!(
        target:"kenvoy::lifecycle",
        "收到第二次终止信号，立即退出；已接受但未完成的写入结果需要核对"
    );
    std::process::exit(130)
}

#[cfg(unix)]
struct Signals {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl Signals {
    fn new() -> io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    async fn recv(&mut self) {
        tokio::select! {
            _ = self.interrupt.recv() => {}
            _ = self.terminate.recv() => {}
        }
    }
}

#[cfg(windows)]
struct Signals {
    ctrl_c: tokio::signal::windows::CtrlC,
    ctrl_break: tokio::signal::windows::CtrlBreak,
    ctrl_close: tokio::signal::windows::CtrlClose,
    ctrl_shutdown: tokio::signal::windows::CtrlShutdown,
}

#[cfg(windows)]
impl Signals {
    fn new() -> io::Result<Self> {
        use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};
        Ok(Self {
            ctrl_c: ctrl_c()?,
            ctrl_break: ctrl_break()?,
            ctrl_close: ctrl_close()?,
            ctrl_shutdown: ctrl_shutdown()?,
        })
    }

    async fn recv(&mut self) {
        tokio::select! {
            _ = self.ctrl_c.recv() => {}
            _ = self.ctrl_break.recv() => {}
            _ = self.ctrl_close.recv() => {}
            _ = self.ctrl_shutdown.recv() => {}
        }
    }
}

#[cfg(not(any(unix, windows)))]
struct Signals;

#[cfg(not(any(unix, windows)))]
impl Signals {
    fn new() -> io::Result<Self> {
        Ok(Self)
    }

    async fn recv(&mut self) {
        let _ = tokio::signal::ctrl_c().await;
    }
}
