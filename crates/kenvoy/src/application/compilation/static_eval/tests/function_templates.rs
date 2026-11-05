use serde_json::json;

use super::super::StaticValue;
use super::{Project, TestResult, element, evaluator};

#[test]
fn default_functions_arrows_aliases_and_typescript_wrappers_extract_the_returned_tree() -> TestResult
{
    for source in [
        "export default function App(){ return (<app name='模板'><page path='/items'/></app>); }",
        "export default function(){ return <app name='模板'><page path='/items'/></app>; }",
        "const App=()=> (<app name='模板'><page path='/items'/></app>); export default App;",
        "const App=()=> { return <app name='模板'><page path='/items'/></app>; }; export default App;",
        "function Template(){return <app name='模板'><page path='/items'/></app>;} const Alias=Template; export {Alias as default};",
        "const App=function Named(){return <app name='模板'><page path='/items'/></app>;}; export default App;",
        "export default (((()=> (<app name='模板'><page path='/items'/></app>)) as unknown) satisfies unknown);",
        "export default function App(){return (<><app name='模板'><page path='/items'/></app></>) satisfies unknown;}",
        "export default (function(){return <app name='模板'><page path='/items'/></app>;});",
        "const App:()=>unknown=()=> <app name='模板'><page path='/items'/></app>; export default App;",
    ] {
        let project = Project::new(&[("app.tsx", source)])?;
        let program = project.program()?;
        let nodes = evaluator(&program).entry(&project.entry()?)?;
        assert_eq!(nodes.len(), 1, "{source}");
        let app = element(&nodes[0]);
        assert_eq!(app.name, "app");
        assert_eq!(app.attributes["name"], StaticValue::Json(json!("模板")));
        let page = element(&app.children[0]);
        assert_eq!(page.name, "page");
        assert_eq!(page.attributes["path"], StaticValue::Json(json!("/items")));
    }
    Ok(())
}

#[test]
fn function_templates_compose_module_and_ordered_local_constants() -> TestResult {
    let project = Project::new(&[(
        "app.tsx",
        r#"
        const unrelated=()=> <resource name='wrong'/>;
        const base={name:'名称',required:true}, limit=10*2;
        const Field=<text {...base} maxLength={limit}/>;
        export default function App(){
            const name='模板', path='/items';
            const fields=<>{Field}<number name='count' min={1}/></>;
            const pageTree=<page path={path}>{fields}</page>;
            const tree=<app name={name}>{pageTree}</app>;
            return tree;
        }
        "#,
    )])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    assert_eq!(app.attributes["name"], StaticValue::Json(json!("模板")));
    assert_eq!(app.children.len(), 1);
    let page = element(&app.children[0]);
    assert_eq!(page.attributes["path"], StaticValue::Json(json!("/items")));
    assert_eq!(page.children.len(), 2);
    let text = element(&page.children[0]);
    assert_eq!(text.name, "text");
    assert_eq!(text.attributes["required"], StaticValue::Json(json!(true)));
    assert_eq!(text.attributes["maxLength"], StaticValue::Json(json!(20)));
    assert_eq!(element(&page.children[1]).name, "number");
    Ok(())
}

#[test]
fn static_reexports_and_namespace_aliases_reach_only_the_selected_template() -> TestResult {
    for entry in [
        "export {default} from './barrel';",
        "import App from './barrel'; export default App;",
        "import * as templates from './template'; export default templates.Template;",
    ] {
        let project = Project::new(&[
            ("app.tsx", entry),
            ("barrel.ts", "export {Alias as default} from './template';"),
            (
                "template.tsx",
                "const unrelated=()=> <app name='错误'/>; export function Template(){return <app name='正确'><page path='/items'/></app>;} export const Alias=Template;",
            ),
        ])?;
        let program = project.program()?;
        let nodes = evaluator(&program).entry(&project.entry()?)?;
        let app = element(&nodes[0]);
        assert_eq!(app.attributes["name"], StaticValue::Json(json!("正确")));
        assert!(app.source.path.ends_with("template.tsx"));
        assert_eq!(app.children.len(), 1);
    }
    Ok(())
}

#[test]
fn ordinary_ui_and_existing_schema_fragments_keep_their_boundaries_inside_function_roots()
-> TestResult {
    let project = Project::new(&[
        (
            "app.tsx",
            "import Fields from './Fields.diu'; import {Card} from './ui'; export default function App(){return <app name='模板'><resource name='items'><Fields/></resource><page path='/items'><Card caption={()=> '动态UI'}/></page></app>;}",
        ),
        (
            "Fields.diu.tsx",
            "const fields=<><text name='name'/></>; export default fields;",
        ),
        (
            "ui.tsx",
            "import {useState} from 'react'; export function Card(){const [count]=useState(0); return <text name='not_schema'>{count}</text>;}",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    let resource = element(&app.children[0]);
    assert_eq!(element(&resource.children[0]).name, "text");
    let card = element(&element(&app.children[1]).children[0]);
    assert_eq!(card.name, "Card");
    assert!(card.children.is_empty());
    assert!(card.module_ref.as_ref().is_some_and(|reference| {
        reference.module.ends_with("ui.tsx") && reference.export == "Card"
    }));
    assert!(matches!(
        card.attributes["caption"],
        StaticValue::Expression(_)
    ));
    Ok(())
}

#[test]
fn bom_offsets_still_locate_anonymous_and_named_function_templates() -> TestResult {
    for source in [
        "\u{feff}export default function(){return <app name='模板'/>;}",
        "\u{feff}export default function App(){const name='模板';return <app name={name}/>;}",
    ] {
        let project = Project::new(&[("app.tsx", source)])?;
        let program = project.program()?;
        let nodes = evaluator(&program).entry(&project.entry()?)?;
        assert_eq!(
            element(&nodes[0]).attributes["name"],
            StaticValue::Json(json!("模板"))
        );
    }
    Ok(())
}

#[test]
fn javascript_jsx_function_templates_use_the_same_entry_contract() -> TestResult {
    let project = Project::new(&[
        ("app.tsx", "export {default} from './template';"),
        (
            "template.jsx",
            "export default function App(){const name='JSX模板';return <app name={name}/>;}",
        ),
    ])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    assert_eq!(
        element(&nodes[0]).attributes["name"],
        StaticValue::Json(json!("JSX模板")),
    );
    Ok(())
}
