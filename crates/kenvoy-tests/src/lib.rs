//! 仅供验收的隔离项目与真实子进程夹具，不被产品 Crate 依赖。

use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(45);
const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
mod windows_job;

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

struct Root {
    path: PathBuf,
    project: PathBuf,
    preserve: AtomicBool,
    sequence: AtomicU64,
}
impl Root {
    fn logs(&self, prefix: &str) -> (PathBuf, PathBuf) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        (
            self.path.join(format!("{prefix}-{sequence}.stdout")),
            self.path.join(format!("{prefix}-{sequence}.stderr")),
        )
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if self.preserve.load(Ordering::Acquire) {
            eprintln!("测试进程未确认回收，保留夹具目录：{}", self.path.display());
            return;
        }
        // 只清理由本夹具通过 create_dir 独占创建、且仍位于临时根下的目录。
        if let (Ok(path), Ok(parent)) = (
            self.path.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) && path.parent() == Some(parent.as_path())
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("kenvoy-integration-"))
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| !metadata.file_type().is_symlink())
        {
            // Windows 的文件系统可能稍后才释放已退出子进程的文件句柄。
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                match fs::remove_dir_all(&path) {
                    Ok(()) => break,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(_) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => {
                        eprintln!("测试夹具目录清理失败，已保留 {}：{error}", path.display());
                        break;
                    }
                }
            }
        }
    }
}

pub struct Fixture {
    root: Arc<Root>,
    binary: PathBuf,
}

impl Fixture {
    pub fn new(label: &str) -> TestResult<Self> {
        if label.is_empty()
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("夹具标签只能包含 ASCII 字母、数字及连字符".into());
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kenvoy-integration-{label}-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        let root = Arc::new(Root {
            project: path.join("app"),
            path,
            preserve: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
        });
        let binary = resolve_binary(&root)?;
        Ok(Self { root, binary })
    }

    pub fn root(&self) -> &Path {
        &self.root.path
    }
    pub fn project(&self) -> &Path {
        &self.root.project
    }
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    pub fn command(&self, args: &[&str], input: &str) -> TestResult<Output> {
        self.command_with(args, input, DEFAULT_TIMEOUT)
    }

    pub fn command_with(
        &self,
        args: &[&str],
        input: &str,
        timeout: Duration,
    ) -> TestResult<Output> {
        let mut command = Command::new(&self.binary);
        command.current_dir(self.root()).args(args);
        output(&self.root, &mut command, input, timeout)
    }

    pub fn create(&self) -> TestResult<()> {
        let result = self.command(&["create", "app"], "")?;
        require_success(&result)?;
        let config = self.project().join("kenvoy.toml");
        let original = fs::read_to_string(&config)?;
        if !original.contains("port = 3000") {
            return Err("新建项目配置缺少预期的端口设置".into());
        }
        fs::write(config, original.replace("port = 3000", "port = 0"))?;
        Ok(())
    }

    pub fn start(&self, mode: &str) -> TestResult<Server> {
        self.start_with(mode, &[], DEFAULT_TIMEOUT, &BTreeMap::new())
    }

    pub fn start_with(
        &self,
        mode: &str,
        args: &[&str],
        timeout: Duration,
        env: &BTreeMap<String, String>,
    ) -> TestResult<Server> {
        if !matches!(mode, "dev" | "run") {
            return Err("服务模式必须为 dev 或 run".into());
        }
        let (stdout, stderr) = self.root.logs("server");
        let mut command = Command::new(&self.binary);
        command
            .current_dir(self.root())
            .args(["--project", "app", mode])
            .args(args)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(create_log(&stdout)?)
            .stderr(create_log(&stderr)?);
        let process = ProcessGuard::spawn(&mut command)?;
        let mut server = Server {
            process,
            root: self.root.clone(),
            stdout,
            stderr,
            origin: String::new(),
        };
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = server.process.try_wait()? {
                return Err(format!("Kenvoy 服务提前退出 {status}：{}", server.logs()?).into());
            }
            if let Some(origin) = extract_origin(&read_text(&server.stdout)?) {
                server.origin = origin;
                return Ok(server);
            }
            if Instant::now() >= deadline {
                return Err(format!("Kenvoy 服务启动超时：{}", server.logs()?).into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

pub struct Server {
    process: ProcessGuard,
    root: Arc<Root>,
    stdout: PathBuf,
    stderr: PathBuf,
    origin: String,
}
impl Server {
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn logs(&self) -> TestResult<String> {
        Ok(format!(
            "{}{}",
            read_text(&self.stdout)?,
            read_text(&self.stderr)?
        ))
    }
    pub fn stop(&mut self) -> TestResult<()> {
        match self.process.stop(Duration::from_secs(10)) {
            Ok(_) => Ok(()),
            Err(error) => {
                self.root.preserve.store(true, Ordering::Release);
                Err(error)
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn require_success(output: &Output) -> TestResult<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "CLI 退出 {}：{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn resolve_binary(root: &Root) -> TestResult<PathBuf> {
    let binary = if let Some(path) = std::env::var_os("KENVOY_BINARY") {
        PathBuf::from(path)
    } else {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("无法定位 Workspace")?;
        let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        command.current_dir(workspace).args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ]);
        let result = output(root, &mut command, "", Duration::from_secs(30))?;
        require_success(&result)?;
        let metadata: serde_json::Value = serde_json::from_slice(&result.stdout)?;
        Path::new(
            metadata["target_directory"]
                .as_str()
                .ok_or("Cargo metadata 缺少 target_directory")?,
        )
        .join("debug")
        .join(if cfg!(windows) {
            "kenvoy.exe"
        } else {
            "kenvoy"
        })
    };
    if !binary.is_file() {
        return Err(format!("未找到真实 Kenvoy Binary：{}。请先执行 cargo build -p kenvoy，或设置 KENVOY_BINARY；测试不会自动构建产品。",binary.display()).into());
    }
    Ok(binary.canonicalize()?)
}

fn create_log(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}
fn read_bytes(path: &Path) -> TestResult<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path)?
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > OUTPUT_LIMIT {
        return Err("测试子进程输出超过 8 MiB 限额".into());
    }
    Ok(data)
}
fn read_text(path: &Path) -> TestResult<String> {
    Ok(String::from_utf8_lossy(&read_bytes(path)?).into_owned())
}

fn output(
    root: &Root,
    command: &mut Command,
    input: &str,
    timeout: Duration,
) -> TestResult<Output> {
    if input.len() > 4096 {
        return Err("测试命令标准输入不能超过 4096 字节".into());
    }
    let (stdout, stderr) = root.logs("command");
    command
        .stdin(Stdio::piped())
        .stdout(create_log(&stdout)?)
        .stderr(create_log(&stderr)?);
    let mut process = ProcessGuard::spawn(command)?;
    if let Some(mut stdin) = process.child_mut().stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let status = match process.wait_timeout(timeout) {
        Ok(status) => status,
        Err(error) => {
            if process.stop(Duration::from_secs(5)).is_err() {
                root.preserve.store(true, Ordering::Release);
            }
            return Err(error);
        }
    };
    let redact = |bytes: Vec<u8>| {
        if input.trim().is_empty() {
            return bytes;
        }
        String::from_utf8_lossy(&bytes)
            .replace(input.trim(), "[已脱敏标准输入]")
            .into_bytes()
    };
    Ok(Output {
        status,
        stdout: redact(read_bytes(&stdout)?),
        stderr: redact(read_bytes(&stderr)?),
    })
}

fn extract_origin(output: &str) -> Option<String> {
    for suffix in output
        .match_indices("http://127.0.0.1:")
        .map(|(offset, _)| &output[offset..])
    {
        let port: String = suffix["http://127.0.0.1:".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if port.parse::<u16>().is_ok_and(|port| port > 0) {
            return Some(format!("http://127.0.0.1:{port}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    fn wait_file(path: &Path) -> TestResult<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(value) = fs::read_to_string(path)
                && !value.is_empty()
            {
                return Ok(value);
            }
            if Instant::now() >= deadline {
                return Err("进程树夹具握手超时".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn process_tree_helper() -> TestResult {
        let Some(mode) = std::env::var_os("KENVOY_PROCESS_TREE_MODE") else {
            return Ok(());
        };
        let root =
            PathBuf::from(std::env::var_os("KENVOY_PROCESS_TREE_ROOT").ok_or("缺少夹具路径")?);
        if mode == "parent" {
            wait_file(&root.join("admitted"))?;
            let mut command = Command::new(std::env::current_exe()?);
            command
                .args(["--exact", "tests::process_tree_helper", "--nocapture"])
                .env("KENVOY_PROCESS_TREE_MODE", "descendant")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let descendant = command.spawn()?;
            wait_file(&root.join("listener"))?;
            // 故意先退出父进程；后代归外层 Job/进程组所有，用于验证失联父进程清理。
            std::mem::forget(descendant);
            std::process::exit(0);
        }
        if mode == "descendant" {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            fs::write(root.join("listener"), listener.local_addr()?.to_string())?;
            // 即使测试夹具失效也有兜底上限，不留下永久后台进程。
            thread::sleep(Duration::from_secs(30));
            drop(listener);
        }
        Ok(())
    }

    #[test]
    fn owned_process_tree_survives_parent_exit_only_until_guard_cleanup() -> TestResult {
        let fixture = Fixture::new("process-tree")?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", "tests::process_tree_helper", "--nocapture"])
            .env("KENVOY_PROCESS_TREE_MODE", "parent")
            .env("KENVOY_PROCESS_TREE_ROOT", fixture.root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut process = ProcessGuard::spawn(&mut command)?;
        fs::write(fixture.root().join("admitted"), "ready")?;
        let address: std::net::SocketAddr = wait_file(&fixture.root().join("listener"))?.parse()?;
        assert!(process.wait_timeout(Duration::from_secs(5))?.success());
        assert!(
            TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok(),
            "父进程退出时后代应仍持有监听端口"
        );
        process.stop(Duration::from_secs(5))?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Guard 必须关闭失联后代的监听端口"
            );
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}
