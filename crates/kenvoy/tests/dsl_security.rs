//! 从真实 CLI 检查声明边界和权限投影；当前不启动 HTTP 或数据库。

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
    fn new(app: &str) -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-dsl-security-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        fs::create_dir(path.join("src"))?;
        fs::write(path.join("kenvoy.toml"), "[project]\nname='安全验证'\n")?;
        fs::write(path.join("src/app.tsx"), app)?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, source: &str) -> Result {
        fs::write(self.0.join("src").join(name), source)?;
        Ok(())
    }
    fn run(&self, command: &str) -> Result<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_kenvoy"))
            .arg("--project")
            .arg(&self.0)
            .arg(command)
            .output()?)
    }
    fn reject(&self, code: &str) -> Result {
        let output = self.run("check")?;
        assert!(
            !output.status.success(),
            "不应接受：{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !self.0.join("data/app.sqlite").exists(),
            "check 不得创建数据库"
        );
        Ok(())
    }
    fn inspect(&self) -> Result<Value> {
        let output = self.run("inspect")?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
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

#[test]
fn reserved_routes_and_ambiguous_spellings_are_rejected_before_runtime() -> Result {
    for path in [
        "/api",
        "/api/customers",
        "/admin",
        "/_kenvoy/state",
        "/double//slash",
        "/customers/",
        "/a/../b",
        "/a/./b",
        "/x%2fy",
        "/x?admin=true",
        "/x#fragment",
        "/:id",
        "/{id}",
        "/wild*",
        "/back\\slash",
    ] {
        let source = format!(
            "export default <page path={{{}}}/>;",
            serde_json::to_string(path)?
        );
        Fixture::new(&source)?.reject("KV2009")?;
    }
    let valid = Fixture::new("export default <app name='路径'><page path='/API'/><page path='/客户'/><page path='/'/></app>;")?.inspect()?;
    assert_eq!(valid["pages"].as_array().ok_or("缺少页面")?.len(), 3);
    Ok(())
}

#[test]
fn resource_names_and_fields_cannot_select_internal_tables_or_sql_identifiers() -> Result {
    for name in [
        "sqlite_master",
        "sqlite_sequence",
        "kenvoy_users",
        "_kenvoy_users",
        "auth",
        "actions",
        "id",
        "created_at",
        "a;drop_table",
        "users--",
        "users.name",
        "MixedCase",
    ] {
        let source = format!(
            "export default <app name='标识'><resource name={{{}}}><text name='name'/></resource></app>;",
            serde_json::to_string(name)?
        );
        Fixture::new(&source)?.reject("KV2006")?;
    }
    for name in [
        "id",
        "updated_at",
        "sqlite_secret",
        "name;DROP TABLE users",
        "__proto__",
    ] {
        let source = format!(
            "export default <page path='/customers'><text name={{{}}}/></page>;",
            serde_json::to_string(name)?
        );
        Fixture::new(&source)?.reject("KV2006")?;
    }
    Ok(())
}

#[test]
fn public_create_forms_do_not_grant_read_update_delete_or_union_their_fields() -> Result {
    let application = Fixture::new(r#"export default <app name="权限"><resource name="customers"><text name="name" required/><text name="phone"/><text name="note"/></resource><page path="/customers"><form resource="customers" fields={['name','phone']}/><form resource="customers" fields={['name','note']}/><table resource="customers"/></page><page path="/members" permission="user"><form resource="customers" fields={['name']}/></page></app>;"#)?.inspect()?;
    let permissions = application["permissions"].as_array().ok_or("缺少权限")?;
    let public: Vec<_> = permissions
        .iter()
        .filter(|permission| permission["role"] == "anonymous")
        .collect();
    assert_eq!(public.len(), 2);
    assert!(public.iter().all(|permission| permission["operation"] == "create" && permission["form"].is_string()));
    assert_eq!(public[0]["fields"], serde_json::json!(["name", "phone"]));
    assert_eq!(public[1]["fields"], serde_json::json!(["name", "note"]));
    assert_ne!(public[0]["form"], public[1]["form"]);
    let ordinary: Vec<_> = permissions
        .iter()
        .filter(|permission| permission["role"] == "user")
        .collect();
    assert_eq!(ordinary.len(), 1);
    assert_eq!(ordinary[0]["operation"], "create");
    let resource_only = Fixture::new("export default <app name='仅资源'><resource name='items'><text name='name'/></resource></app>;")?.inspect()?;
    assert!(
        resource_only["permissions"]
            .as_array()
            .ok_or("缺少权限")?
            .iter()
            .all(|permission| permission["role"] == "admin")
    );
    Ok(())
}

#[test]
fn unknown_attributes_impossible_ranges_and_invalid_defaults_are_diagnostics() -> Result {
    for (field, code) in [
        ("<text name='name' requird/>", "KV2002"),
        ("<number name='age' integer min={0.1} max={0.9}/>", "KV2007"),
        ("<text name='name' required maxLength={0}/>", "KV2007"),
        ("<number name='age' integer default={1.5}/>", "KV2008"),
        ("<text name='name' required default={null}/>", "KV2008"),
        ("<date name='day' default='2023-02-29'/>", "KV2008"),
        (
            "<datetime name='time' default='2024-01-01T00:00:00.1234Z'/>",
            "KV2008",
        ),
        (
            "<select name='status' default='missing'><option value='active'/></select>",
            "KV2008",
        ),
    ] {
        let source = format!("export default <page path='/customers'>{field}</page>;");
        Fixture::new(&source)?.reject(code)?;
    }
    let application = Fixture::new("export default <page path='/customers'><text name='none'/><text name='nullish' default={null}/></page>;")?.inspect()?;
    let fields = application["resources"][0]["fields"]
        .as_array()
        .ok_or("缺少字段")?;
    assert!(fields[0].get("default").is_none());
    assert_eq!(fields[1].get("default"), Some(&Value::Null));
    Ok(())
}

#[test]
fn imported_ui_with_builtin_spelling_cannot_be_interpreted_as_schema() -> Result {
    let fixture = Fixture::new(
        "import {text} from './ui'; export default <app name='组件'><resource name='items'><text name='sneaky'/></resource></app>;",
    )?;
    fixture.write("ui.tsx", "export function text() { return <div/>; }")?;
    fixture.reject("KV2001")?;
    let root =
        Fixture::new("import {app} from './ui'; export default <app name='不能冒充声明'/>;")?;
    root.write("ui.tsx", "export function app() { return <div/>; }")?;
    root.reject("KV2001")?;
    Ok(())
}

#[test]
fn ui_transitive_server_imports_and_unmodelled_dynamic_loading_are_rejected() -> Result {
    for widget in [
        "import {secret} from './bridge'; export function Card(){return <div>{secret}</div>;}",
        "export function Card(){const loading=import('./secret.server');return <div/>;}",
        "export function Card(){const loading=require('./secret.server');return <div/>;}",
    ] {
        let fixture = Fixture::new(
            "import {Card} from './ui'; export default <page path='/customers'><Card/></page>;",
        )?;
        fixture.write("ui.tsx", widget)?;
        fixture.write("bridge.ts", "export {secret} from './secret.server';")?;
        fixture.write("secret.server.ts", "export const secret='不应进入浏览器';")?;
        fixture.reject("KV2101")?;
    }
    let types = Fixture::new(
        "import {Card} from './ui'; export default <page path='/customers'><Card/></page>;",
    )?;
    types.write("ui.tsx", "import type {Kind} from './secret.server'; export function Card(props:Kind){return <div/>;}")?;
    types.write("secret.server.ts", "export interface Kind { value:string }")?;
    assert!(
        types.run("check")?.status.success(),
        "纯类型导入不会成为客户端运行依赖"
    );
    Ok(())
}
