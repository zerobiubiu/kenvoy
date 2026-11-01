use crate::{Fixture, ProcessGuard, TestResult};
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

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
    let root = PathBuf::from(std::env::var_os("KENVOY_PROCESS_TREE_ROOT").ok_or("缺少夹具路径")?);
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
