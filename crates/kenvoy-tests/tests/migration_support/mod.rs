//! 正式迁移验收共用真实 CLI 与只读 SQLite 断言；所有写入均限本测试拥有的夹具。

use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

use kenvoy_tests::{Fixture, TestResult, require_success};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

pub(super) fn project(label: &str) -> TestResult<Fixture> {
    let fixture = Fixture::new(label)?;
    fixture.create()?;
    Ok(fixture)
}

pub(super) fn command(fixture: &Fixture, args: &[&str]) -> TestResult<Output> {
    let mut command = vec!["--project", "app"];
    command.extend_from_slice(args);
    fixture.command(&command, "")
}

pub(super) fn migration(fixture: &Fixture, args: &[&str]) -> TestResult<Value> {
    let mut command_args = vec!["migrate"];
    command_args.extend_from_slice(args);
    let output = command(fixture, &command_args)?;
    require_success(&output)?;
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub(super) fn reject(output: &Output) -> TestResult<String> {
    assert!(!output.status.success(), "不应接受该操作");
    let diagnostic = String::from_utf8(output.stderr.clone())?;
    assert!(!diagnostic.trim().is_empty(), "拒绝操作时必须报告原因");
    Ok(diagnostic)
}

pub(super) fn seal(
    fixture: &Fixture,
    description: &str,
    rules: Option<&Path>,
) -> TestResult<Value> {
    let mut args = vec!["generate", "--description", description];
    let rules = rules
        .map(|path| path.to_str().ok_or("迁移规则路径不是 UTF-8"))
        .transpose()?;
    if let Some(path) = rules {
        args.extend(["--rules", path]);
    }
    let generated = migration(fixture, &args)?;
    let checksum = generated["confirmation_checksum"]
        .as_str()
        .ok_or("候选缺少确认值")?;
    let sealed = migration(fixture, &["seal", "--confirm", checksum])?;
    assert_eq!(sealed["status"], "sealed");
    assert_eq!(sealed["position"]["checksum"], checksum);
    Ok(sealed)
}

pub(super) fn apply(fixture: &Fixture) -> TestResult<Value> {
    let status = migration(fixture, &["status"])?;
    if let Some(checksum) = status["confirmation_checksum"].as_str() {
        migration(fixture, &["apply", "--confirm", checksum])
    } else {
        migration(fixture, &["apply"])
    }
}

pub(super) fn db(fixture: &Fixture) -> PathBuf {
    fixture.project().join("data/app.sqlite")
}

pub(super) fn source(fixture: &Fixture) -> PathBuf {
    fixture.project().join("src/app.tsx")
}

pub(super) fn append_fields(fixture: &Fixture, fields: &str) -> TestResult<String> {
    let previous = fs::read_to_string(source(fixture))?;
    assert!(previous.contains("</resource>"), "模板必须有资源声明");
    fs::write(
        source(fixture),
        previous.replacen("</resource>", &format!("{fields}</resource>"), 1),
    )?;
    Ok(previous)
}

pub(super) fn rules(fixture: &Fixture, name: &str, value: &Value) -> TestResult<PathBuf> {
    let path = fixture.root().join(format!("{name}.json"));
    fs::write(&path, serde_json::to_vec_pretty(value)?)?;
    Ok(path)
}

pub(super) fn data_rules(fixture: &Fixture, name: &str, sql: &str) -> TestResult<PathBuf> {
    rules(fixture, name, &json!({"format":1,"after_sql":[sql]}))
}

pub(super) fn read_db(path: &Path) -> TestResult<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

pub(super) fn scalar(path: &Path, sql: &str) -> TestResult<i64> {
    Ok(read_db(path)?.query_row(sql, [], |row| row.get(0))?)
}

pub(super) fn insert_customer(fixture: &Fixture, name: &str, age: i64) -> TestResult {
    Connection::open(db(fixture))?.execute(
        "INSERT INTO customers(name,phone,age) VALUES(?1,'保留电话',?2)",
        (name, age),
    )?;
    Ok(())
}

pub(super) fn history(fixture: &Fixture) -> TestResult<Vec<Value>> {
    migration(fixture, &["history"])?["applied"]
        .as_array()
        .cloned()
        .ok_or_else(|| "正式迁移历史不是数组".into())
}

pub(super) fn manifest_path(fixture: &Fixture) -> PathBuf {
    fixture.project().join("migrations/manifest.json")
}

pub(super) fn manifest(fixture: &Fixture) -> TestResult<Value> {
    Ok(serde_json::from_slice(&fs::read(manifest_path(fixture))?)?)
}

pub(super) fn step_file(fixture: &Fixture, index: usize) -> TestResult<PathBuf> {
    let manifest = manifest(fixture)?;
    let file = manifest["steps"][index]["file"]
        .as_str()
        .ok_or("链清单缺少迁移文件")?;
    Ok(fixture.project().join("migrations").join(file))
}

pub(super) fn copy_owned_directory(from: &Path, to: &Path) -> TestResult {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        assert!(!kind.is_symlink(), "测试夹具不能复制符号链接");
        let destination = to.join(entry.file_name());
        if kind.is_dir() {
            copy_owned_directory(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
