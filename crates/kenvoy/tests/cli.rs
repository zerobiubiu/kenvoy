//! 运行真实 Kenvoy Binary 检查项目创建、诊断、模型与路径保护。

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("kenvoy-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&root)?;
        Ok(Self(root))
    }
    fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .output()?)
    }
    fn project(&self) -> PathBuf {
        self.0.join("customer-app")
    }
    fn create(&self) -> Result<()> {
        let output = self.run(&["create", "customer-app"])?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // 仅移除本测试在系统临时目录创建的唯一目录。
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn create_check_and_inspect_use_real_tsx_without_creating_a_database() -> Result {
    let fixture = Fixture::new()?;
    fixture.create()?;
    let checked = fixture.run(&["--project", "customer-app/src", "check"])?;
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8(checked.stdout)?.contains("检查通过"));
    let inspected = fixture.run(&["--project", "customer-app", "inspect"])?;
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let application: serde_json::Value = serde_json::from_slice(&inspected.stdout)?;
    assert_eq!(application["format_version"], 1);
    assert_eq!(application["resources"][0]["name"], "customers");
    assert_eq!(application["resources"][0]["fields"][0]["max_length"], 100);
    assert_eq!(application["pages"][0]["path"], "/customers");
    let permissions = application["permissions"].as_array().ok_or("缺少权限")?;
    assert!(
        permissions
            .iter()
            .any(|rule| rule["role"] == "anonymous" && rule["operation"] == "create")
    );
    assert!(
        !permissions
            .iter()
            .any(|rule| rule["role"] == "anonymous" && rule["operation"] != "create")
    );
    assert!(!fixture.project().join("data/app.sqlite").exists());
    let resource = fixture.run(&["--project", "customer-app", "inspect", "customers"])?;
    assert!(resource.status.success());
    assert!(String::from_utf8(resource.stdout)?.contains("不代表当前用户可调用"));
    let generic = fixture.run(&["--project", "customer-app", "inspect", "--generic"])?;
    assert!(generic.status.success());
    let generic: serde_json::Value = serde_json::from_slice(&generic.stdout)?;
    assert!(
        !generic["modules"][0]["nodes"]
            .as_array()
            .ok_or("缺少节点")?
            .is_empty()
    );
    Ok(())
}

#[test]
fn create_never_overwrites_existing_files_and_compile_failure_has_diagnostics() -> Result {
    let fixture = Fixture::new()?;
    fixture.create()?;
    let marker = fixture.project().join("keep.txt");
    fs::write(&marker, "保留用户内容")?;
    assert!(!fixture.run(&["create", "customer-app"])?.status.success());
    assert_eq!(fs::read_to_string(marker)?, "保留用户内容");
    fs::write(
        fixture.project().join("src/app.tsx"),
        "export default <app><page>",
    )?;
    let output = fixture.run(&["--project", "customer-app", "check"])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("KE1001") && stderr.contains("app.tsx") && stderr.contains("建议"));
    let structured = fixture.run(&["--json-logs", "--project", "customer-app", "check"])?;
    assert!(!structured.status.success());
    let entries = String::from_utf8(structured.stderr)?
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let diagnostic = entries
        .iter()
        .find(|entry| entry["fields"]["code"] == "KE1001")
        .ok_or("JSON 日志缺少语法诊断")?;
    assert_eq!(diagnostic["fields"]["line"], 1);
    assert!(
        diagnostic["fields"]["column"]
            .as_u64()
            .is_some_and(|column| column > 0)
    );
    assert!(
        diagnostic["fields"]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("app.tsx"))
    );
    assert!(
        diagnostic["fields"]["suggestion"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn configuration_errors_and_output_data_overlap_are_fatal() -> Result {
    let fixture = Fixture::new()?;
    fixture.create()?;
    let config = fixture.project().join("kenvoy.toml");
    let original = fs::read_to_string(&config)?;
    fs::write(
        &config,
        original.replace("out_dir = \"dist\"", "out_dir = \"data/build\""),
    )?;
    let output = fixture.run(&["--project", "customer-app", "check"])?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("彼此独立"));
    fs::write(config, "[server]\nportt = 3000\n")?;
    let output = fixture.run(&["--project", "customer-app", "check"])?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("portt"));
    assert!(
        !fixture
            .project()
            .join(Path::new("data").join("build"))
            .exists()
    );
    Ok(())
}

#[test]
fn check_validates_real_record_json_and_rejects_duplicates_without_writing() -> Result {
    let fixture = Fixture::new()?;
    fixture.create()?;
    let input = fixture.0.join("record.json");
    fs::write(&input, r#"{"name":"张三","phone":"13800000000","age":25}"#)?;
    let args = [
        "--project",
        "customer-app",
        "check",
        "--resource",
        "customers",
        "--input",
        "record.json",
    ];
    let output = fixture.run(&args)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)?.contains("未写入数据库"));
    fs::write(&input, r#"{"name":"张三","age":-1}"#)?;
    assert!(!fixture.run(&args)?.status.success());
    fs::write(&input, r#"{"name":"张三","name":"重复"}"#)?;
    assert!(!fixture.run(&args)?.status.success());
    fs::write(&input, r#"{"phone":"新电话"}"#)?;
    let mut patch = args.to_vec();
    patch.push("--patch");
    assert!(fixture.run(&patch)?.status.success());
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
