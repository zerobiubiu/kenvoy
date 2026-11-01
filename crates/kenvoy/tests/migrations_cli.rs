//! 通过真实 CLI 验证迁移预览、持久化、确认、备份与历史读取。

use rusqlite::Connection;
use serde_json::Value;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root =
            std::env::temp_dir().join(format!("kenvoy-migrate-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        fixture.success(&["create", "app"])?;
        Ok(fixture)
    }
    fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .output()?)
    }
    fn success(&self, args: &[&str]) -> Result<Output> {
        let output = self.run(args)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }
    fn migrate(&self, args: &[&str]) -> Result<Value> {
        let mut command = vec!["--project", "app", "migrate"];
        command.extend(args);
        Ok(serde_json::from_slice(&self.success(&command)?.stdout)?)
    }
    fn db(&self) -> PathBuf {
        self.0.join("app/data/app.sqlite")
    }
    fn source(&self) -> PathBuf {
        self.0.join("app/src/app.tsx")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // 只清理本测试创建的唯一临时目录。
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn migration_cli_preserves_data_and_history_across_processes() -> Result {
    let fixture = Fixture::new()?;
    let plan = fixture.migrate(&["status"])?;
    assert_eq!(plan["from_version"], 0);
    assert_eq!(plan["destructive"], false);
    assert!(!fixture.db().exists());
    assert_eq!(fixture.migrate(&["history"])?, serde_json::json!([]));
    assert!(!fixture.db().exists());
    let applied = fixture.migrate(&["apply"])?;
    assert_eq!(applied["database"]["journal_mode"], "wal");
    assert_eq!(applied["database"]["foreign_keys"], true);
    {
        let connection = Connection::open(fixture.db())?;
        connection.execute(
            "INSERT INTO customers(name,phone,age) VALUES(?1,?2,?3)",
            ("张三", "13800000000", 25),
        )?;
    }
    fixture.migrate(&["apply"])?;
    assert_eq!(
        fixture
            .migrate(&["history"])?
            .as_array()
            .ok_or("缺少历史")?
            .len(),
        1
    );
    let original = fs::read_to_string(fixture.source())?;
    fs::write(
        fixture.source(),
        original.replace("</resource>", "<text name=\"notes\" index /></resource>"),
    )?;
    let plan = fixture.migrate(&["status"])?;
    assert_eq!(plan["destructive"], false);
    fixture.migrate(&["apply"])?;
    let connection = Connection::open(fixture.db())?;
    let record: (String, i64, Option<String>) =
        connection.query_row("SELECT name,age,notes FROM customers", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    assert_eq!(record, ("张三".into(), 25, None));
    drop(connection);
    assert_eq!(
        fixture
            .migrate(&["history"])?
            .as_array()
            .ok_or("缺少历史")?
            .len(),
        2
    );
    fs::write(fixture.source(), "export default <app>")?;
    assert_eq!(
        fixture
            .migrate(&["history"])?
            .as_array()
            .ok_or("缺少历史")?
            .len(),
        2
    );
    assert!(
        !fixture
            .run(&["--project", "app", "migrate", "apply"])?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn destructive_cli_requires_current_checksum_and_retains_backup() -> Result {
    let fixture = Fixture::new()?;
    fixture.migrate(&["apply"])?;
    let source = fs::read_to_string(fixture.source())?;
    let connection = Connection::open(fixture.db())?;
    connection.execute(
        "INSERT INTO customers(name,phone,age) VALUES('张三','保留在备份',25)",
        [],
    )?;
    drop(connection);
    let line = source
        .lines()
        .find(|line| line.contains("name=\"phone\""))
        .ok_or("缺少电话字段")?;
    fs::write(fixture.source(), source.replace(line, ""))?;
    let plan = fixture.migrate(&["status"])?;
    assert_eq!(plan["destructive"], true);
    let failed = fixture.run(&["--project", "app", "migrate", "apply"])?;
    assert!(!failed.status.success());
    assert!(String::from_utf8(failed.stderr)?.contains("checksum"));
    assert!(
        !fixture
            .run(&[
                "--project",
                "app",
                "migrate",
                "apply",
                "--confirm",
                "过期计划"
            ])?
            .status
            .success()
    );
    let checksum = plan["checksum"].as_str().ok_or("缺少校验值")?;
    fixture.migrate(&["apply", "--confirm", checksum])?;
    let history = fixture.migrate(&["history"])?;
    let backup = history[1]["backup"].as_str().ok_or("缺少备份")?;
    let connection = Connection::open(backup)?;
    let phone: String =
        connection.query_row("SELECT phone FROM customers", [], |row| row.get(0))?;
    assert_eq!(phone, "保留在备份");
    drop(connection);
    let connection = Connection::open(fixture.db())?;
    let name: String = connection.query_row("SELECT name FROM customers", [], |row| row.get(0))?;
    assert_eq!(name, "张三");
    connection.execute_batch("ALTER TABLE customers ADD COLUMN unmanaged TEXT;")?;
    drop(connection);
    let failed = fixture.run(&["--project", "app", "migrate", "status"])?;
    assert!(!failed.status.success());
    assert!(String::from_utf8(failed.stderr)?.contains("漂移"));
    Ok(())
}
