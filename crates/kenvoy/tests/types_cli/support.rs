use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

pub(super) struct Fixture(pub PathBuf);
impl Fixture {
    pub fn empty() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let number = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kenvoy-types-cli-{}-{stamp}-{number}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    pub fn new() -> Result<Self> {
        let fixture = Self::empty()?;
        success(&fixture.run(&["create", "app"])?)?;
        Ok(fixture)
    }
    pub fn project(&self) -> PathBuf {
        self.0.join("app")
    }
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .output()?)
    }
    pub fn without_tools(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .env("PATH", self.0.join("no-programs"))
            .args(args)
            .output()?)
    }
    pub fn types(&self) -> Result<Output> {
        self.without_tools(&["--project", "app", "types"])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub(super) fn success(output: &Output) -> Result {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub(super) fn reject(output: &Output) -> Result<String> {
    assert!(!output.status.success(), "不应覆盖用户配置、声明或链接边界");
    let diagnostic = String::from_utf8(output.stderr.clone())?;
    assert!(!diagnostic.trim().is_empty(), "拒绝必须提供可操作诊断");
    Ok(diagnostic)
}

pub(super) fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            assert!(!kind.is_symlink(), "文件快照只用于无链接的自有夹具");
            if kind.is_dir() {
                visit(root, &entry.path(), files)?;
            } else {
                files.insert(
                    entry.path().strip_prefix(root)?.into(),
                    fs::read(entry.path())?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}
