//! 测试自有进程树的启动、期限与回收。

#[cfg(unix)]
use std::process::Stdio;
use std::{
    io,
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use crate::TestResult;
#[cfg(windows)]
use crate::windows_job;

/// 仅管理由本实例直接创建的进程及其子进程；不会按进程名称批量清理。
pub struct ProcessGuard {
    child: Child,
    stopped: bool,
    #[cfg(windows)]
    job: windows_job::Job,
}

impl ProcessGuard {
    pub fn spawn(command: &mut Command) -> TestResult<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        let job = windows_job::Job::new()?;
        let child = command.spawn()?;
        let guard = Self {
            child,
            stopped: false,
            #[cfg(windows)]
            job,
        };
        #[cfg(windows)]
        let guard = {
            let mut guard = guard;
            if let Err(error) = guard.job.assign(&guard.child) {
                let _ = guard.child.kill();
                let _ = guard.wait_timeout(Duration::from_secs(5));
                return Err(error.into());
            }
            guard
        };
        Ok(guard)
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait_timeout(&mut self, timeout: Duration) -> TestResult<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(
                    io::Error::new(io::ErrorKind::TimedOut, "测试子进程超过允许时间").into(),
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn stop(&mut self, timeout: Duration) -> TestResult<ExitStatus> {
        if self.stopped {
            return self
                .child
                .try_wait()?
                .ok_or_else(|| "已停止进程缺少退出状态".into());
        }
        // 这是验收夹具的终止手段，不将其记为 Ctrl+C 优雅退出证据。
        #[cfg(windows)]
        {
            self.job.terminate(timeout)?;
        }
        #[cfg(unix)]
        {
            let mut terminator = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{}", self.child.id())])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let deadline = Instant::now() + timeout;
            while terminator.try_wait()?.is_none() {
                if Instant::now() >= deadline {
                    let _ = terminator.kill();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "测试进程组终止命令超过允许时间",
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        let status = self.wait_timeout(timeout)?;
        self.stopped = true;
        Ok(status)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.stop(Duration::from_secs(5));
    }
}
