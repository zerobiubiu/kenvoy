//! 本机账号初始化使用受控标准输入，验证真实 Binary 和持久化状态。

use std::{
    error::Error,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-admin-cli-{}-{stamp}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        let fixture = Self(path);
        let output = fixture.run(&["create", "app"], None)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(fixture)
    }
    fn run(&self, args: &[&str], password: Option<&str>) -> Result<Output> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(password) = password {
            let mut input = child.stdin.take().ok_or("缺少标准输入")?;
            input.write_all(password.as_bytes())?;
        } else {
            drop(child.stdin.take());
        }
        Ok(child.wait_with_output()?)
    }
    fn admin(&self, command: &str, name: &str, password: &str) -> Result<Output> {
        self.run(
            &[
                "--project",
                "app",
                "admin",
                command,
                "--username",
                name,
                "--password-stdin",
            ],
            Some(password),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn administrator_initialization_is_explicit_and_cannot_overwrite_existing_accounts() -> Result {
    let fixture = Fixture::new()?;
    let secret = "temporary-test-password-9284";
    assert!(
        !fixture
            .admin("add-user", "visitor", secret)?
            .status
            .success()
    );
    assert!(!fixture.admin("init", "admin", "short")?.status.success());
    let output = fixture.admin("init", "admin", &format!("{secret}\r\n"))?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("已创建账号"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
    assert!(
        !fixture
            .admin("init", "replacement", secret)?
            .status
            .success()
    );
    assert!(
        fixture
            .admin("add-user", "visitor", secret)?
            .status
            .success()
    );
    assert!(
        !fixture
            .admin("add-user", "visitor", secret)?
            .status
            .success()
    );
    let inspected = fixture.run(&["--project", "app", "inspect"], None)?;
    assert!(inspected.status.success());
    let public_ir: String = String::from_utf8(inspected.stdout)?;
    assert!(!public_ir.contains(secret));
    assert!(!public_ir.contains("password_hash"));
    assert!(fixture.0.join("app/data/app.sqlite").is_file());
    Ok(())
}

#[test]
fn password_input_requires_explicit_pipe_mode_and_enforces_bounded_utf8() -> Result {
    let fixture = Fixture::new()?;
    let no_flag = fixture.run(
        &["--project", "app", "admin", "init", "--username", "admin"],
        None,
    )?;
    assert!(!no_flag.status.success());
    assert!(String::from_utf8_lossy(&no_flag.stderr).contains("--password-stdin"));
    assert!(
        !fixture
            .admin("init", "admin", &"a".repeat(1024))?
            .status
            .success()
    );
    assert!(
        !fixture
            .admin("init", "admin", "abcdefghijkl\u{0000}")?
            .status
            .success()
    );
    assert!(
        fixture
            .admin("init", "admin", "correct-test-password-2")?
            .status
            .success()
    );
    Ok(())
}
