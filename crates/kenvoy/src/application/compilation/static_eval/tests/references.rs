use serde_json::Value;

use super::super::StaticValue;
use super::{Project, TestResult, element, evaluator};

#[test]
fn cross_module_constants_reexports_schema_fragments_and_ui_references_are_distinct() -> TestResult
{
    let project = Project::new(&[
        (
            "app.tsx",
            "import Fields from './barrel'; import { Card, label } from './ui'; import * as constants from './constants'; export default <app name={constants.title}><resource name='customers'><Fields /></resource><page path='/customers'><Card value={3} caption={label()} onClick={() => 2} /></page></app>;",
        ),
        ("barrel.ts", "export { default } from './Fields.diu';"),
        (
            "Fields.diu.tsx",
            "import { limit } from './constants'; export default <><text name='name' maxLength={limit}/><text name='phone'/></>;",
        ),
        (
            "constants.ts",
            "export const title = '客户管理', limit = 100;",
        ),
        (
            "ui.tsx",
            "import { useState } from 'react'; export function label(){ return '名称'; } export function Card() { const [count] = useState(0); return <text name='not_schema'>{count}</text>; }",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    let resource = element(&app.children[0]);
    assert_eq!(resource.children.len(), 2);
    assert!(
        element(&resource.children[0])
            .source
            .path
            .ends_with("Fields.diu.tsx")
    );
    let card = element(&element(&app.children[1]).children[0]);
    assert!(card.children.is_empty(), "普通 UI 函数体不能进入 Schema");
    assert!(card.module_ref.as_ref().is_some_and(|reference| {
        reference.module.ends_with("ui.tsx") && reference.export == "Card"
    }));
    assert!(matches!(
        card.attributes["onClick"],
        StaticValue::Expression(_)
    ));
    assert!(matches!(
        card.attributes["caption"],
        StaticValue::Expression(_)
    ));
    Ok(())
}

#[test]
fn exported_function_and_arrow_handlers_remain_module_references() -> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import { calculate, asyncCalc } from './logic.server'; export default <app><action handler={calculate}/><action handler={asyncCalc}/></app>;",
        ),
        (
            "logic.server.ts",
            "export function calculate(value: number) { return value * 2; } export const asyncCalc = async (value: number) => value + 1;",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    for child in &element(&nodes[0]).children {
        assert!(matches!(
            element(child).attributes["handler"],
            StaticValue::Reference(_)
        ));
    }
    Ok(())
}

#[test]
fn runtime_imports_preserve_barrels_aliases_and_namespace_export_names() -> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import First, { View as Alias, run as handle, label } from './barrel'; import * as all from './barrel'; const local=handle; export default <app name={label}><page><First onClick={handle}/><Alias/><all.View/></page><action handler={all['run']}/><action handler={local}/></app>;",
        ),
        (
            "barrel.ts",
            "import './side-effects'; export { default, Card as View } from './ui'; export { calculate as run } from './logic'; export { label } from './constants';",
        ),
        ("side-effects.ts", "console.log('保留门面运行依赖');"),
        (
            "ui.tsx",
            "export function Card(){ return <div/>; } export default Card;",
        ),
        (
            "logic.ts",
            "export function calculate(value: number){ return value * 2; }",
        ),
        (
            "constants.ts",
            "export const label='静态常量仍追踪最终声明';",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    assert_eq!(
        app.attributes["name"],
        StaticValue::Json(Value::String("静态常量仍追踪最终声明".into()))
    );
    let page = element(&app.children[0]);
    for (node, name) in page.children.iter().zip(["default", "View", "View"]) {
        let reference = element(node).module_ref.as_ref().ok_or("组件引用缺失")?;
        assert_eq!(
            reference.module,
            project.0.join("barrel.ts").canonicalize()?
        );
        assert_eq!(reference.export, name);
        assert!(!reference.external);
    }
    let callback = &element(&page.children[0]).attributes["onClick"];
    for value in [
        callback,
        &element(&app.children[1]).attributes["handler"],
        &element(&app.children[2]).attributes["handler"],
    ] {
        let StaticValue::Reference(reference) = value else {
            panic!("预期函数引用")
        };
        assert!(reference.module.ends_with("barrel.ts"));
        assert_eq!(reference.export, "run");
    }
    Ok(())
}

#[test]
fn external_reexports_keep_local_facade_and_local_exports_keep_their_module() -> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import { Card } from './barrel'; export function Local(){ return <div/>; } export function onClick(){} export default <app><Card callback={Card}/><Local onClick={onClick}/></app>;",
        ),
        (
            "barrel.ts",
            "export { createElement as Card } from 'react';",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    let first = element(&app.children[0]);
    let reference = first.module_ref.as_ref().ok_or("缺少组件引用")?;
    assert!(reference.module.ends_with("barrel.ts"));
    assert_eq!(reference.export, "Card");
    assert!(!reference.external);
    assert_eq!(
        first.attributes["callback"],
        StaticValue::Reference(reference.clone())
    );
    let local = element(&app.children[1]);
    assert!(local.module_ref.as_ref().is_some_and(|reference| {
        reference.module.ends_with("app.tsx") && reference.export == "Local"
    }));
    assert!(
        matches!(&local.attributes["onClick"], StaticValue::Reference(reference) if reference.module.ends_with("app.tsx") && reference.export == "onClick")
    );
    Ok(())
}

#[test]
fn nested_namespace_runtime_references_are_rejected_instead_of_losing_the_facade() -> TestResult {
    for source in [
        "import * as all from './barrel'; export default <app><all.ui.Card/></app>;",
        "import { ui } from './barrel'; export default <app><ui.Card/></app>;",
        "import { ui } from './barrel'; export default <app><action handler={ui.Card}/></app>;",
    ] {
        let project = Project::new(&[
            ("app.tsx", source),
            ("barrel.ts", "export * as ui from './ui';"),
            ("ui.tsx", "export function Card(){ return <div/>; }"),
        ])?;
        let program = project.program()?;
        let error = evaluator(&program)
            .entry(&project.entry()?)
            .expect_err("应诊断未支持的成员链");
        assert_eq!(error.code, "KV1104");
        assert!(error.message.contains("嵌套模块成员"));
    }
    Ok(())
}

#[test]
fn imported_lowercase_components_cannot_masquerade_as_builtin_fields() -> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import { text } from './ui'; export default <app><resource name='items'><text name='x'/></resource></app>;",
        ),
        ("ui.tsx", "export function text(){return <input/>;}"),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let field = element(&element(&element(&nodes[0]).children[0]).children[0]);
    assert_eq!(field.name, "text");
    assert!(field.module_ref.as_ref().is_some_and(
        |reference| reference.export == "text" && reference.module.ends_with("ui.tsx")
    ));
    Ok(())
}
