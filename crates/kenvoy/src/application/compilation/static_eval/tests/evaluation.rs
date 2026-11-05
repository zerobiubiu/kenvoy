use serde_json::Value;

use super::super::StaticValue;
use super::{Project, TestResult, element, evaluator};

#[test]
fn literal_tree_and_const_operators_are_evaluated_without_scanning_other_jsx() -> TestResult {
    let project = Project::new(&[(
        "app.tsx",
        r#"
        const unrelated = () => <resource name="wrong" />;
        const limit = 20 * 5, properties = { required: true, maxLength: limit };
        const choice = true ? 'customers' : unknown;
        export default (<app name={`系统-${choice}`}>{false && <resource name="unused"/>}{true && <page path="/customers"><text name="name" {...properties}/><number name="age" min={-0} integer /></page>}</app>) satisfies unknown;
    "#,
    )])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    assert_eq!(nodes.len(), 1);
    let app = element(&nodes[0]);
    assert_eq!(
        app.attributes["name"],
        StaticValue::Json(Value::String("系统-customers".into()))
    );
    let page = element(&app.children[0]);
    let field = element(&page.children[0]);
    assert_eq!(
        field.attributes["maxLength"],
        StaticValue::Json(Value::from(100))
    );
    assert_eq!(page.children.len(), 2);
    Ok(())
}

#[test]
fn static_arrays_objects_members_and_ordered_object_spread_are_supported() -> TestResult {
    let project = Project::new(&[(
        "app.tsx",
        "const defaults={label:'旧', required:true}; const config={...defaults,label:'名称'}; const values=[1,...[2,3]]; export default <app name={config.label}><page path='/test'><number name='value' min={values[0]} max={values.length}/></page></app>;",
    )])?;
    let program = project.program()?;
    let nodes = evaluator(&program).entry(&project.entry()?)?;
    let app = element(&nodes[0]);
    assert_eq!(
        app.attributes["name"],
        StaticValue::Json(Value::String("名称".into()))
    );
    let number = element(&element(&app.children[0]).children[0]);
    assert_eq!(number.attributes["min"], StaticValue::Json(Value::from(1)));
    assert_eq!(number.attributes["max"], StaticValue::Json(Value::from(3)));
    Ok(())
}

#[test]
fn duplicate_attributes_and_conflicting_jsx_spreads_are_rejected() -> TestResult {
    for source in [
        "export default <app name='one' name='two'/>;",
        "const defaults={name:'one'}; export default <app {...defaults} name='two'/>;",
    ] {
        let project = Project::new(&[("app.tsx", source)])?;
        let program = project.program()?;
        let error = evaluator(&program)
            .entry(&project.entry()?)
            .expect_err("应拒绝重复属性");
        assert_eq!(error.code, "KV1105");
    }
    Ok(())
}
