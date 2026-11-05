use crate::domain::model::{Operation, Role};

use super::{Fixture, TestResult};

#[test]
fn function_template_builds_the_same_resource_permissions_and_declared_routes() -> TestResult {
    let application = Fixture::new(
        "export default function App(){const name='客户模板',fields=<text name='name' label='姓名' required/>;return <app name={name}><resource name='customers'>{fields}</resource><page path='/customers'><form resource='customers'/></page></app>;}",
        &[],
    )?.valid()?;
    assert_eq!(application.name, "客户模板");
    assert_eq!(application.resources.len(), 1);
    let customers = application.resource("customers").ok_or("缺少声明资源")?;
    assert_eq!(customers.fields.len(), 1);
    assert_eq!(customers.fields[0].name, "name");
    assert_eq!(customers.fields[0].label, "姓名");
    assert_eq!(application.pages.len(), 1);
    assert_eq!(application.pages[0].path, "/customers");
    let public = application
        .permissions
        .iter()
        .filter(|policy| policy.role == Role::Anonymous)
        .collect::<Vec<_>>();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].operation, Operation::Create);
    assert_eq!(public[0].fields, ["name"]);
    Ok(())
}

#[test]
fn arrow_page_templates_preserve_the_declared_path_and_default_application_name() -> TestResult {
    for source in [
        "export default ()=> <page path='/items'><text name='name'/></page>;",
        "const Page=()=>{const path='/items';return <page path={path}><text name='name'/></page>;};export default Page;",
        "function Page(){return <page path='/items'><text name='name'/></page>;}const Alias=Page;export {Alias as default};",
    ] {
        let application = Fixture::new(source, &[])?.valid()?;
        assert_eq!(application.name, "测试应用");
        assert_eq!(application.pages.len(), 1);
        assert_eq!(application.pages[0].path, "/items");
        assert_eq!(application.resources.len(), 1);
        assert_eq!(application.resources[0].name, "items");
        assert_eq!(application.resources[0].fields[0].name, "name");
    }
    Ok(())
}

#[test]
fn function_templates_do_not_invent_routes_or_accept_arbitrary_html_roots() -> TestResult {
    for source in [
        "export default function App(){return <div><text name='not_schema'/></div>;}",
        "export default ()=> <><page path='/one'/><page path='/two'/></>;",
        "export default function App(){return <></>;}",
        "export default ()=> <article/>;",
    ] {
        let result = Fixture::new(source, &[])?.analyze()?;
        assert!(result.application.is_none());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|error| error.code == "KV2001")
        );
    }
    Ok(())
}

#[test]
fn statically_reexported_function_template_reaches_the_model_consumer() -> TestResult {
    let application = Fixture::new(
        "export {default} from './barrel';",
        &[
            ("barrel.ts", "export {App as default} from './template';"),
            ("template.tsx", "export function App(){const path='/items';return <page path={path}><text name='name'/></page>;}"),
        ],
    )?.valid()?;
    assert_eq!(application.pages.len(), 1);
    assert_eq!(application.pages[0].path, "/items");
    assert_eq!(application.resources[0].fields.len(), 1);
    Ok(())
}
