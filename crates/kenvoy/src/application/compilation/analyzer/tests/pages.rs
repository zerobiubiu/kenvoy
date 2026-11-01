use serde_json::Value;

use crate::domain::model::{
    ApplicationIR, ComponentDefinition, ModuleReferenceKind, Operation, Role,
};

use super::{Fixture, TestResult};

#[test]
fn customer_acceptance_builds_one_model_with_controlled_public_create() -> TestResult {
    let fixture = Fixture::new(
        r#"export default <app name="客户管理系统"><resource name="customers"><text name="name" label="客户姓名" required maxLength={100}/><text name="phone" label="联系电话"/><number name="age" label="年龄" integer min={0}/></resource><page path="/customers" title="客户登记"><form resource="customers" mode="create"/></page></app>;"#,
        &[],
    )?;
    let application = fixture.valid()?;
    assert_eq!(application.name, "客户管理系统");
    assert_eq!(application.resources.len(), 1);
    assert_eq!(application.pages.len(), 1);
    let resource = application.resource("customers").ok_or("客户资源缺失")?;
    assert_eq!(resource.fields.len(), 3);
    assert_eq!(
        resource.field("name").and_then(|field| field.max_length),
        Some(100)
    );
    let public: Vec<_> = application
        .permissions
        .iter()
        .filter(|permission| permission.role == Role::Anonymous)
        .collect();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].operation, Operation::Create);
    assert_eq!(public[0].fields, ["name", "phone", "age"]);
    assert_eq!(
        application
            .permissions
            .iter()
            .filter(|permission| permission.role == Role::Admin)
            .count(),
        5
    );
    let encoded = serde_json::to_value(&application)?;
    assert_eq!(encoded["resources"][0]["fields"][0]["kind"], "text");
    assert_eq!(
        serde_json::from_value::<ApplicationIR>(encoded)?,
        application
    );
    Ok(())
}

#[test]
fn implicit_resources_resolve_after_all_pages_and_reuse_explicit_schema() -> TestResult {
    let app = r#"export default <app name="复用"><page path="/list"><table resource="customers"/></page><page path="/customers"><text name="name" required/></page><page path="/customers/new" resource="customers"><text name="name" label="名称" required/></page></app>;"#;
    let application = Fixture::new(app, &[])?.valid()?;
    assert_eq!(application.resources.len(), 1);
    assert_eq!(application.pages.len(), 3);
    let ComponentDefinition::Form { labels: first, .. } = &application.pages[1].components[0]
    else {
        return Err("缺少页面简写表单".into());
    };
    let ComponentDefinition::Form { labels: second, .. } = &application.pages[2].components[0]
    else {
        return Err("缺少页面简写表单".into());
    };
    assert_eq!(first.get("name").map(String::as_str), Some("name"));
    assert_eq!(second.get("name").map(String::as_str), Some("名称"));
    assert_eq!(application.resources[0].fields[0].label, "name");
    assert!(
        !application
            .permissions
            .iter()
            .any(|permission| permission.role == Role::Anonymous
                && permission.operation == Operation::List)
    );
    let explicit = r#"export default <app name="显式"><page path="/customers"><text name="name" required/></page><resource name="customers"><text name="name" label="显式名称" required/></resource></app>;"#;
    let application = Fixture::new(explicit, &[])?.valid()?;
    assert_eq!(application.resources[0].fields[0].label, "显式名称");
    Ok(())
}

#[test]
fn schema_components_expand_but_ui_function_jsx_never_becomes_schema() -> TestResult {
    let fixture = Fixture::new(
        r#"import Fields from './fields.diu'; import {Card} from './card'; export default <app name="组件"><resource name="customers"><Fields/></resource><page path="/customers"><Card title="客户"/><form resource="customers"/></page></app>;"#,
        &[
            (
                "fields.diu.tsx",
                "export default <><text name='name' required/><number name='age' integer min={0}/></>;",
            ),
            (
                "card.tsx",
                "export function Card(){return <text name='not_a_field'/>;}",
            ),
        ],
    )?;
    let application = fixture.valid()?;
    assert_eq!(application.resources[0].fields.len(), 2);
    assert!(application.resources[0].field("not_a_field").is_none());
    assert_eq!(application.module_references.len(), 1);
    assert_eq!(
        application.module_references[0].kind,
        ModuleReferenceKind::ClientComponent
    );
    assert!(matches!(
        application.pages[0].components[0],
        ComponentDefinition::Custom { .. }
    ));
    Ok(())
}

#[test]
fn semantic_failures_never_publish_a_partial_application() -> TestResult {
    let cases = [
        (
            r#"export default <app name="x"><resource name="items"><text name="same"/><text name="same"/></resource></app>;"#,
            "KV2005",
        ),
        (
            r#"export default <page path="/items"><number name="age" min={10} max={0}/></page>;"#,
            "KV2007",
        ),
        (
            r#"export default <page path="/items"><number name="age" integer default={1.2}/></page>;"#,
            "KV2008",
        ),
        (
            r#"export default <page path="/items"><number name="age" maxLength={3}/></page>;"#,
            "KV2002",
        ),
        (
            r#"export default <page path="/items"><text name="name" required="yes"/></page>;"#,
            "KV2004",
        ),
        (
            r#"export default <page path="/items/new"><text name="name"/></page>;"#,
            "KV2007",
        ),
        (
            r#"export default <page path="/items"><form resource="missing"/></page>;"#,
            "KV2011",
        ),
        (
            r#"export default <app name="x"><page path="/items"/><page path="/items"/></app>;"#,
            "KV2009",
        ),
        (
            r#"export default <app name="x"><resource name="items"><text name="name"/></resource><page path="/items"><number name="name"/></page></app>;"#,
            "KV2010",
        ),
        (
            r#"export default <><page path="/one"/><page path="/two"/></>;"#,
            "KV2001",
        ),
        (r#"export default <></>;"#, "KV2001"),
    ];
    for (source, expected) in cases {
        let result = Fixture::new(source, &[])?.analyze()?;
        assert!(result.application.is_none(), "应阻断：{source}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == expected),
            "期望 {expected}，得到 {:?}",
            result.diagnostics
        );
    }
    Ok(())
}

#[test]
fn form_field_policies_remain_separate_and_invalid_versions_are_rejected() -> TestResult {
    let application = Fixture::new(r#"export default <app name="表单"><resource name="items"><text name="name" required/><text name="phone"/><text name="note"/></resource><page path="/items"><form resource="items" fields={['name','phone']}/><form resource="items" fields={['name','note']}/></page></app>;"#, &[])?.valid()?;
    let policies: Vec<_> = application
        .permissions
        .iter()
        .filter(|policy| policy.role == Role::Anonymous)
        .collect();
    assert_eq!(policies.len(), 2);
    assert_eq!(policies[0].fields, ["name", "phone"]);
    assert_eq!(policies[1].fields, ["name", "note"]);
    assert_ne!(policies[0].form, policies[1].form);
    let mut value = serde_json::to_value(application)?;
    value["format_version"] = Value::from(99);
    assert!(serde_json::from_value::<ApplicationIR>(value).is_err());
    Ok(())
}
