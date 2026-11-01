//! 同一真实项目跨越 KEngine 公共模型与 Kenvoy Binary 的业务解释边界。

use std::fs;

use kengine::{AnalyzeOptions, Engine, NodeKind, ProgramIR};
use kenvoy_tests::{Fixture, TestResult, require_success};
use serde_json::Value;

#[test]
fn module_graph_and_generic_ir_feed_the_real_host_dsl() -> TestResult {
    let fixture = Fixture::new("engine-host")?;
    fixture.create()?;
    fs::write(
        fixture.project().join("src/fields.diu.tsx"),
        "export default <><text name='name' required/><number name='age' integer min={0}/></>;",
    )?;
    fs::write(
        fixture.project().join("src/app.tsx"),
        "import Fields from './fields.diu'; export default <app name='跨 Crate 验收'><resource name='people'><Fields/></resource><page path='/people'><form resource='people'/></page></app>;",
    )?;
    let generic = Engine::new().analyze_project(
        fixture.project(),
        AnalyzeOptions {
            entry_points: vec!["src/app.tsx".into()],
            ..Default::default()
        },
    )?;
    assert!(!generic.has_errors(), "通用引擎应接受合法跨模块 TSX");
    assert_eq!(generic.modules.len(), 2);
    assert!(generic.modules.iter().any(|module| module.nodes.iter().any(
        |node| matches!(&node.kind,NodeKind::Element(element) if element.name.text()=="text")
    )));
    let roundtrip: ProgramIR = serde_json::from_slice(&serde_json::to_vec(&generic)?)?;
    assert_eq!(roundtrip.modules.len(), generic.modules.len());
    let checked = fixture.command(&["--project", "app", "check"], "")?;
    require_success(&checked)?;
    let inspected = fixture.command(&["--project", "app", "inspect"], "")?;
    require_success(&inspected)?;
    let model: Value = serde_json::from_slice(&inspected.stdout)?;
    assert_eq!(model["resources"][0]["name"], "people");
    assert_eq!(model["resources"][0]["fields"][0]["name"], "name");
    assert_eq!(model["resources"][0]["fields"][0]["required"], true);
    assert_eq!(model["resources"][0]["fields"][1]["integer"], true);
    assert_eq!(model["pages"][0]["components"][0]["resource"], "people");
    let inspected_generic = fixture.command(&["--project", "app", "inspect", "--generic"], "")?;
    require_success(&inspected_generic)?;
    let host_generic: ProgramIR = serde_json::from_slice(&inspected_generic.stdout)?;
    assert_eq!(
        serde_json::to_value(host_generic)?,
        serde_json::to_value(generic)?,
        "CLI 应消费同一 KEngine 公共模型"
    );
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}

#[test]
fn engine_accepts_generic_jsx_while_host_rejects_invalid_business_declarations() -> TestResult {
    let fixture = Fixture::new("engine-host-boundary")?;
    fixture.create()?;
    fs::write(
        fixture.project().join("src/app.tsx"),
        "export default <app name='职责边界'><resource name='items'><text name='name'/><text name='name'/></resource></app>;",
    )?;
    let generic = Engine::new().analyze_project(
        fixture.project(),
        AnalyzeOptions {
            entry_points: vec!["src/app.tsx".into()],
            ..Default::default()
        },
    )?;
    assert!(!generic.has_errors(), "重复业务字段不应污染通用语言引擎");
    let checked = fixture.command(&["--project", "app", "check"], "")?;
    assert!(!checked.status.success());
    let diagnostic = String::from_utf8(checked.stderr)?;
    assert!(
        diagnostic.contains("KV2005")
            && diagnostic.contains("name")
            && diagnostic.contains("app.tsx")
    );
    assert!(!fixture.project().join("data/app.sqlite").exists());
    assert!(!fixture.project().join("dist").exists());
    Ok(())
}
