//! 隔离项目、服务与有界命令输出。

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{ProcessGuard, TestResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(45);
const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
