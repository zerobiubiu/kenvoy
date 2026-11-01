use super::create::CUSTOMER_APP;
use super::*;
use kengine::AnalysisSession;
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-project-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn write(
        &self,
        name: &str,
        value: &str,
    ) -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?
        }
        fs::write(&path, value)?;
        Ok(path.canonicalize()?)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn stylesheet_noop_and_changes_reuse_code_while_ui_changes_parse_one_module()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", "")?;
    scratch.write(
        "src/app.tsx",
        "import { Widget } from './Widget'; export default <Widget />;",
    )?;
    let widget = scratch.write("src/Widget.tsx", "import './style.css'; import { Fragment } from 'react/jsx-runtime'; export function Widget() { return <Fragment>甲</Fragment>; }")?;
    scratch.write("src/style.css", "body { color: red; }")?;
    let project = Project::load(&scratch.0)?;
    let mut session = AnalysisSession::new();
    let first = project.analyze(&mut session)?;
    assert!(
        !first.program.has_errors(),
        "{:?}",
        first.program.diagnostics
    );
    assert_eq!(first.program.modules.len(), 2);
    let noop = project.analyze(&mut session)?;
    assert!(noop.stats.parsed.is_empty());
    assert_eq!(noop.stats.reused.len(), 2);
    scratch.write("src/style.css", "body { color: blue; }")?;
    let css = project.analyze(&mut session)?;
    assert_eq!(css.program, noop.program);
    assert!(css.stats.parsed.is_empty());
    scratch.write("src/Widget.tsx", "import './style.css'; import { Fragment } from 'react/jsx-runtime'; export function Widget() { return <Fragment>乙</Fragment>; }")?;
    let changed = project.analyze(&mut session)?;
    assert_eq!(changed.stats.parsed, vec![widget]);
    assert_eq!(changed.stats.reused.len(), 1);
    Ok(())
}

#[test]
fn relative_external_is_validated_per_importer_and_removed_stylesheets_fail()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", "")?;
    scratch.write(
        "src/app.tsx",
        "import './a/Widget'; import './b/Widget'; export default <root />;",
    )?;
    scratch.write(
        "src/a/Widget.tsx",
        "import './style.css'; export const a = 1;",
    )?;
    scratch.write(
        "src/b/Widget.tsx",
        "import './style.css'; export const b = 2;",
    )?;
    scratch.write("src/a/style.css", "a {}")?;
    let project = Project::load(&scratch.0)?;
    let mut session = AnalysisSession::new();
    let error = project
        .analyze(&mut session)
        .expect_err("另一个目录不存在的同名CSS不能被external放行");
    assert!(
        matches!(error, Error::Dsl(ref diagnostics) if diagnostics.iter().any(|diagnostic| diagnostic.code == "KV3006" && diagnostic.source.path.ends_with("b/Widget.tsx")))
    );
    let b = scratch.write("src/b/style.css", "b {}")?;
    let valid = project.analyze(&mut session)?;
    assert!(!valid.program.has_errors());
    fs::remove_file(b)?;
    assert!(project.analyze(&mut session).is_err());
    Ok(())
}

#[test]
fn reserved_paths_and_nonexistent_case_aliases_cannot_overlap_data_or_sources()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let scratch = Scratch::new()?;
    let root = scratch.0.canonicalize()?;
    for name in [
        ".git",
        "safe/.git/objects",
        "node_modules/output",
        ".kenvoy/cache",
        "data/file.sqlite:secret",
        "data/NUL.sqlite",
        "data/CON",
        "data/trailing.",
        "data/trailing ",
    ] {
        assert!(safe_path(&root, Path::new(name)).is_err(), "{name}");
    }
    assert!(safe_path(&root, Path::new("space name/data.sqlite")).is_ok());
    scratch.write("kenvoy.toml", "[build]\nout_dir='data/dist'")?;
    assert!(Project::load(&scratch.0).is_err());
    #[cfg(windows)]
    {
        scratch.write("kenvoy.toml", "[build]\nout_dir='DATA/dist'")?;
        assert!(Project::load(&scratch.0).is_err());
    }
    Ok(())
}

#[test]
fn package_allowlist_cannot_encode_relative_or_absolute_module_escape()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let scratch = Scratch::new()?;
    scratch.write("kenvoy.toml", "")?;
    let project = Project::load(&scratch.0)?;
    for package in [
        "./local",
        "../outside",
        "C:\\private",
        "@scope/name/deep",
        "node:fs",
    ] {
        scratch.write(
            "package.json",
            &serde_json::json!({"dependencies":{package:"1.0.0"}}).to_string(),
        )?;
        assert!(project.external_packages().is_err(), "{package}");
    }
    scratch.write(
        "package.json",
        r#"{"dependencies":{"@scope/name":"1.0.0","react-library":"2.0.0"}}"#,
    )?;
    assert!(project.external_packages()?.contains("@scope/name"));
    Ok(())
}

#[test]
fn create_preserves_existing_empty_and_nonempty_targets()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let scratch = Scratch::new()?;
    let target = scratch.0.join("app");
    fs::create_dir(&target)?;
    assert!(create(&target).is_err());
    assert_eq!(fs::read_dir(&target)?.count(), 0);
    fs::write(target.join("user.txt"), "用户内容")?;
    assert!(create(&target).is_err());
    assert_eq!(fs::read_to_string(target.join("user.txt"))?, "用户内容");
    let fresh = scratch.0.join("fresh");
    create(&fresh)?;
    assert!(fresh.join("kenvoy.toml").is_file());
    assert_eq!(fs::read_to_string(fresh.join("src/app.tsx"))?, CUSTOMER_APP);
    Ok(())
}
