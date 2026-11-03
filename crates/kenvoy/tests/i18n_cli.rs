//! 原生词典命令只读取自有临时项目；用真实 Binary 验证提取、配置和旧格式边界。

use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

#[path = "i18n_cli/config.rs"]
mod config;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-i18n-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        let fixture = Self(path);
        success(&fixture.run(&["create", "app"])?)?;
        Ok(fixture)
    }
    fn project(&self) -> PathBuf {
        self.0.join("app")
    }
    fn run(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .current_dir(&self.0)
            .args(args)
            .output()?)
    }
    fn command(&self, args: &[&str]) -> Result<Output> {
        let mut command = vec!["--project", "app"];
        command.extend_from_slice(args);
        self.run(&command)
    }
    fn config(&self) -> Result<toml::Value> {
        Ok(toml::from_str(&fs::read_to_string(
            self.project().join("kenvoy.toml"),
        )?)?)
    }
    fn write_config(&self, config: &toml::Value) -> Result {
        fs::write(
            self.project().join("kenvoy.toml"),
            toml::to_string_pretty(config)?,
        )?;
        Ok(())
    }
    fn extract(&self) -> Result<Value> {
        let before = snapshot(&self.project())?;
        let output = self.command(&["i18n", "extract"])?;
        success(&output)?;
        assert_eq!(snapshot(&self.project())?, before, "提取不能改写项目文件");
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn success(output: &Output) -> Result {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
fn reject(output: &Output, expected: &str) -> Result {
    assert!(!output.status.success(), "不应接受该词典或配置");
    let diagnostic = String::from_utf8(output.stderr.clone())?;
    assert!(diagnostic.contains(expected), "{diagnostic}");
    Ok(())
}
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            assert!(!kind.is_symlink(), "自有夹具不应包含符号链接");
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

#[test]
fn create_provides_native_bilingual_messages_and_keeps_application_ir_format_one() -> Result {
    let fixture = Fixture::new()?;
    let config = fixture.config()?;
    assert_eq!(config["i18n"]["default_locale"].as_str(), Some("zh-CN"));
    assert_eq!(
        config["i18n"]["locales"].as_array().ok_or("缺少语言列表")?,
        &[toml::Value::from("zh-CN"), toml::Value::from("en-US")]
    );
    let defaults = fixture.extract()?;
    let zh: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("locales/zh-CN.json"))?)?;
    let en: Value =
        serde_json::from_slice(&fs::read(fixture.project().join("locales/en-US.json"))?)?;
    assert_eq!(defaults["app.name"], "客户管理系统");
    assert_eq!(defaults["pages./customers.title"], "客户登记");
    assert_eq!(zh["resources.customers.fields.name.label"], "客户姓名");
    assert_eq!(en["resources.customers.fields.name.label"], "Customer name");
    assert_eq!(en["pages./customers.title"], "Customer registration");
    success(&fixture.command(&["check"])?)?;
    success(&fixture.command(&["i18n", "check"])?)?;
    let inspected = fixture.command(&["inspect"])?;
    success(&inspected)?;
    let ir: Value = serde_json::from_slice(&inspected.stdout)?;
    assert_eq!(ir["format_version"], 1);
    assert_eq!(ir["resources"][0]["name"], "customers");
    assert!(!fixture.project().join("data/app.sqlite").exists());
    assert!(!fixture.project().join("dist").exists());
    Ok(())
}

#[test]
fn extract_covers_native_display_text_without_reading_database_or_translating_data() -> Result {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.project().join("src/card.tsx"),
        "export function Card(props: unknown) { return <div/>; }",
    )?;
    let source = r#"import {Card} from './card';
export default <app name="提取验收">
  <resource name="customers">
    <text name="name" label="姓名" required default="业务默认值"/>
    <select name="category" label="类别"><option value="business-code" label="甲类"/></select>
  </resource>
  <page path="/customers" title="登记页面">
    <group label="登记分组"><form resource="customers"/><image src="/private-image.png" alt="图片说明"/>页面说明</group>
    <Card href="https://private.example/data" payload="不应提取的组件数据"/>
  </page>
  <page path="/customers/alias" resource="customers" title="别名页面"><text name="name" label="登记姓名" required default="业务默认值"/><select name="category" label="类别"><option value="business-code" label="甲类"/></select></page>
  <navigation label="站点导航"><group label="客户分组"><item path="/customers" label="登记入口"/></group></navigation>
</app>;"#;
    fs::write(fixture.project().join("src/app.tsx"), source)?;
    // 非 SQLite 文件会使任何真实数据库打开失败；提取必须完全忽略它。
    fs::write(
        fixture.project().join("data/app.sqlite"),
        b"not-a-sqlite-database\0owned-marker",
    )?;
    let extracted = fixture.extract()?;
    for (key, value) in [
        ("app.name", "提取验收"),
        ("resources.customers.label", "customers"),
        ("resources.customers.fields.name.label", "姓名"),
        (
            "resources.customers.fields.category.options.business-code.label",
            "甲类",
        ),
        ("pages./customers.title", "登记页面"),
        ("text:登记分组", "登记分组"),
        ("text:图片说明", "图片说明"),
        ("text:页面说明", "页面说明"),
        ("text:登记姓名", "登记姓名"),
        ("text:站点导航", "站点导航"),
        ("text:客户分组", "客户分组"),
        ("text:登记入口", "登记入口"),
    ] {
        assert_eq!(extracted[key], value, "缺少展示词条 {key}");
    }
    let values = extracted.as_object().ok_or("提取结果必须为平面词典")?;
    for excluded in [
        "业务默认值",
        "business-code",
        "/private-image.png",
        "https://private.example/data",
        "不应提取的组件数据",
    ] {
        assert!(
            values.values().all(|value| value != excluded),
            "业务值不能被收集：{excluded}"
        );
    }
    fs::write(
        fixture.project().join("src/app.tsx"),
        source.replace(
            "<form resource=\"customers\"/><image src=\"/private-image.png\" alt=\"图片说明\"/>",
            "<image src=\"/private-image.png\" alt=\"图片说明\"/><form resource=\"customers\"/>",
        ),
    )?;
    assert_eq!(fixture.extract()?, extracted, "组件重排不能更换词条键");
    Ok(())
}
