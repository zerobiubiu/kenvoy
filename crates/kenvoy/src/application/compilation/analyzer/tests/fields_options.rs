use serde_json::Value;

use crate::domain::model::{
    ApplicationIR, ComponentDefinition, FieldKind, FileConstraints, ModuleReferenceKind,
};

use super::{Fixture, TestResult};

#[test]
fn files_and_images_produce_complete_ir_without_changing_scalar_serialization() -> TestResult {
    let application = Fixture::new(
        r#"import {View} from './view'; export default <app name='附件'><override name='file' component={View}/><override name='image' component={View}/><resource name='items'><text name='title'/><file name='attachments' label='附件' maxFiles={20} maxSize={1} accept={['image/*','application/pdf']} default={[]}/><file name='other' default={null}/></resource><page path='/items'><form resource='items'/><group label='图片'><image src='https://example.com/a.png' alt='' width={320} height={4096} fit='cover'/><image src='/images/a.png' alt='示例'/></group></page></app>;"#,
        &[("view.tsx", "export function View(){return <div/>;}")],
    )?.valid()?;
    let fields = &application.resources[0].fields;
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[1].kind, FieldKind::File);
    assert_eq!(
        fields[1].upload,
        Some(FileConstraints {
            accept: vec!["image/*".into(), "application/pdf".into()],
            max_size: 1,
            max_files: 20,
        })
    );
    assert_eq!(fields[1].default, Some(serde_json::json!([])));
    assert_eq!(fields[2].upload, Some(FileConstraints::default()));
    assert_eq!(fields[2].default, Some(Value::Null));
    assert!(
        !serde_json::to_value(&fields[0])?
            .as_object()
            .ok_or("缺少字段对象")?
            .contains_key("upload")
    );
    assert_eq!(application.component_overrides.len(), 2);
    let ComponentDefinition::Group { children, .. } = &application.pages[0].components[1] else {
        return Err("缺少图片组".into());
    };
    assert!(matches!(&children[0], ComponentDefinition::Image {
        src, alt, width: Some(320), height: Some(4096), fit, ..
    } if src == "https://example.com/a.png" && alt.is_empty() && fit == "cover"));
    assert!(matches!(&children[1], ComponentDefinition::Image {
        width: None, height: None, fit, ..
    } if fit == "contain"));
    let restored: ApplicationIR = serde_json::from_slice(&serde_json::to_vec(&application)?)?;
    assert_eq!(restored, application);
    let implicit = Fixture::new(
        "export default <page path='/items'><group><file name='attachments'/><image src='/a.png' alt='图片'/></group></page>;",
        &[],
    )?.valid()?;
    assert_eq!(implicit.resources[0].fields.len(), 1);
    assert_eq!(implicit.resources[0].fields[0].kind, FieldKind::File);
    Ok(())
}

#[test]
fn file_declarations_reject_invalid_constraints_defaults_and_children() -> TestResult {
    for declaration in [
        "<file name='attachments' maxFiles={0}/>",
        "<file name='attachments' maxFiles={21}/>",
        "<file name='attachments' maxFiles={1.5}/>",
        "<file name='attachments' maxFiles='2'/>",
        "<file name='attachments' maxSize={0}/>",
        "<file name='attachments' maxSize={10485761}/>",
        "<file name='attachments' accept='image/png'/>",
        "<file name='attachments' accept={['.png']}/>",
        "<file name='attachments' accept={['Image/png']}/>",
        "<file name='attachments' accept={['*/*']}/>",
        "<file name='attachments' accept={['image/png','image/png']}/>",
        "<file name='attachments' default={[{}]}/>",
        "<file name='attachments' default='[]'/>",
        "<file name='attachments' required default={[]}/>",
        "<file name='attachments' required default={null}/>",
        "<file name='attachments' unique/>",
        "<file name='attachments' index={false}/>",
        "<file name='attachments' min={0}/>",
        "<file name='attachments' maxLength={1}/>",
        "<file name='attachments'><image src='/a.png' alt=''/></file>",
        "<image src='/a.png' alt=''/>",
    ] {
        let source = format!(
            "export default <app name='非法附件'><resource name='items'>{declaration}</resource></app>;"
        );
        let result = Fixture::new(&source, &[])?.analyze()?;
        assert!(result.application.is_none(), "{declaration}");
        assert!(!result.diagnostics.is_empty(), "{declaration}");
    }
    Ok(())
}

#[test]
fn images_reject_invalid_attributes_urls_and_business_children() -> TestResult {
    for declaration in [
        "<image src='/a.png'/>",
        "<image src='/a.png' alt={null}/>",
        "<image src='/a.png' alt='' width={0}/>",
        "<image src='/a.png' alt='' height={4097}/>",
        "<image src='/a.png' alt='' width={2.5}/>",
        "<image src='/a.png' alt='' fit='fill'/>",
        "<image src='//example.com/a.png' alt=''/>",
        "<image src='https://user:secret@example.com/a.png' alt=''/>",
        "<image src='javascript:alert(1)' alt=''/>",
        "<image src='data:image/png;base64,AA' alt=''/>",
        "<image src='blob:https://example.com/id' alt=''/>",
        "<image src='/a.png' alt=''><file name='attachments'/></image>",
    ] {
        let source = format!("export default <page path='/view'>{declaration}</page>;");
        let result = Fixture::new(&source, &[])?.analyze()?;
        assert!(result.application.is_none(), "{declaration}");
        assert!(!result.diagnostics.is_empty(), "{declaration}");
    }
    Ok(())
}

#[test]
fn option_dependencies_never_treat_file_arrays_as_scalars() -> TestResult {
    for selector in [
        "<select name='choice'><option value='a' when={{attachments:null}}/></select>",
        "<select name='choice'><option value='a' when={{attachments:[]}}/></select>",
        "<select name='choice' optionsSource={{resource:'sources',value:'attachments',label:'title',permission:'anonymous'}}/>",
        "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'attachments',permission:'anonymous'}}/>",
        "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{attachments:'attachments'},permission:'anonymous'}}/>",
        "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{title:'attachments'},permission:'anonymous'}}/>",
        "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{attachments:'title'},permission:'anonymous'}}/>",
    ] {
        let source = format!(
            "export default <app name='联动'><resource name='sources'><text name='title'/><file name='attachments'/></resource><resource name='items'><text name='title'/><file name='attachments'/>{selector}</resource></app>;"
        );
        let result = Fixture::new(&source, &[])?.analyze()?;
        assert!(result.application.is_none(), "{selector}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "KV2015"),
            "{selector}: {:?}",
            result.diagnostics
        );
    }
    Ok(())
}

#[test]
fn linked_options_sources_and_overrides_are_explicit_and_validated() -> TestResult {
    let source = r#"import {SelectView} from './view'; export default <app name='选项'><override name='select' component={SelectView}/><resource name='cities'><text name='code' unique/><text name='name'/><text name='province'/></resource><resource name='customers'><select name='province'><option value='zj'/><option value='js'/></select><select name='city' optionsSource={{resource:'cities',value:'code',label:'name',filters:{province:'province'},permission:'anonymous'}}/><select name='district'><option value='xihu' when={{province:'zj'}}/><option value='gulou' when={{province:'js'}}/></select></resource><page path='/customers'><form resource='customers'/></page></app>;"#;
    let files = [(
        "view.tsx",
        "export function SelectView(props: unknown){return <select/>;}",
    )];
    let app = Fixture::new(source, &files)?.valid()?;
    assert_eq!(app.component_overrides.len(), 1);
    assert_eq!(
        app.module_references[0].kind,
        ModuleReferenceKind::ClientComponent
    );
    let city = app
        .resource("customers")
        .ok_or("缺少资源")?
        .field("city")
        .ok_or("缺少字段")?;
    assert_eq!(
        city.options_source.as_ref().ok_or("缺少来源")?.resource,
        "cities"
    );
    for invalid in [
        source.replace("value:'code'", "value:'name'"),
        source.replace("permission:'anonymous'", "permission:'root'"),
        source.replace("permission:'anonymous'", "permission:'admin'"),
        source.replace(
            "filters:{province:'province'}",
            "filters:{province:'missing'}",
        ),
        source.replace("when={{province:'zj'}}", "when={{district:'xihu'}}"),
        source.replace("component={SelectView}", "component={'SelectView'}"),
        source.replace("name='select' component", "name='unknown' component"),
        source.replace(
            "<resource name='cities'>",
            "<override name='select' component={SelectView}/><resource name='cities'>",
        ),
        source.replace(
            "<form resource='customers'/>",
            "<form resource='customers' fields={['city']}/>",
        ),
    ] {
        assert!(
            Fixture::new(&invalid, &files)?
                .analyze()?
                .application
                .is_none(),
            "应拒绝 {invalid}"
        );
    }
    let mut old = serde_json::to_value(&app)?;
    old.as_object_mut()
        .ok_or("缺少对象")?
        .remove("component_overrides");
    let restored: ApplicationIR = serde_json::from_value(old)?;
    assert!(restored.component_overrides.is_empty());
    Ok(())
}

#[test]
fn all_field_kinds_validate_constraints_options_and_defaults() -> TestResult {
    let application = Fixture::new(r#"export default <app name="字段"><resource name="items"><text name="name" required default="张三" maxLength={4} unique/><textarea name="note" default={null}/><number name="amount" min={0} max={100} integer default={10}/><boolean name="enabled" default={true}/><date name="day" default="2024-02-29"/><datetime name="time" default="2024-02-29T08:00:00+08:00"/><select name="status" default="new" index><option value="new" label="新增"/><option value="done"/></select></resource></app>;"#, &[])?.valid()?;
    let fields = &application.resources[0].fields;
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[1].default, Some(Value::Null));
    assert_eq!(
        fields[5].default,
        Some(Value::String("2024-02-29T00:00:00.000Z".into()))
    );
    let round_trip: ApplicationIR = serde_json::from_slice(&serde_json::to_vec(&application)?)?;
    assert_eq!(round_trip, application);
    assert_eq!(round_trip.resources[0].fields[1].default, Some(Value::Null));
    Ok(())
}

#[test]
fn select_options_reject_nul_without_requiring_a_default_value() -> TestResult {
    for attribute in [r#"{"a\u0000b"}"#, r#"{"\0"}"#] {
        let source = format!(
            "export default <app name='选项验证'><resource name='items'><select name='status'><option value={attribute}/></select></resource></app>;"
        );
        let result = Fixture::new(&source, &[])?.analyze()?;
        assert!(result.application.is_none());
        assert!(result.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "KV2007" && diagnostic.message.contains("NUL")
        }));
    }
    Ok(())
}
