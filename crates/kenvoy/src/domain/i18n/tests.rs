use std::collections::BTreeMap;

use serde_json::json;

use super::{I18nBundle, defaults, validate_locales};
use crate::domain::model::{ApplicationIR, ComponentDefinition};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn legacy_bundle_is_a_valid_single_language_without_project_messages() -> TestResult {
    let bundle = I18nBundle::default();
    bundle.validate()?;
    assert_eq!(bundle.default_locale, "zh-CN");
    assert_eq!(bundle.locales, ["zh-CN"]);
    assert!(bundle.messages["zh-CN"].is_empty());
    Ok(())
}

#[test]
fn bundles_reject_unknown_versions_missing_catalogs_and_undeclared_languages() {
    let mut bundle = I18nBundle {
        format_version: 2,
        ..Default::default()
    };
    assert!(bundle.validate().is_err());
    bundle.format_version = 1;
    bundle.locales.push("en-US".into());
    assert!(bundle.validate().is_err());
    bundle.messages.insert("en-US".into(), BTreeMap::new());
    assert!(bundle.validate().is_ok());
    bundle.messages.insert("fr-FR".into(), BTreeMap::new());
    assert!(bundle.validate().is_err());
    bundle.messages.remove("fr-FR");
    bundle
        .messages
        .get_mut("en-US")
        .unwrap()
        .insert("".into(), "empty".into());
    assert!(bundle.validate().is_err());
}

#[test]
fn bundle_decoding_rejects_duplicate_languages_messages_and_non_string_values() {
    for messages in [
        r#"{"zh-CN":{},"zh-CN":{}}"#,
        r#"{"zh-CN":{"label":"甲","label":"乙"}}"#,
        r#"{"zh-CN":{"label":"甲","\u006cabel":"乙"}}"#,
        r#"{"zh-CN":{"label":{"nested":"甲"}}}"#,
        r#"{"zh-CN":{"label":1}}"#,
    ] {
        let encoded = format!(
            r#"{{"format_version":1,"default_locale":"zh-CN","locales":["zh-CN"],"messages":{messages}}}"#
        );
        assert!(
            serde_json::from_str::<I18nBundle>(&encoded).is_err(),
            "{messages}"
        );
    }
}

#[test]
fn locale_rules_allow_portable_intl_tags_and_reject_ambiguous_or_unsafe_identifiers() {
    for locale in [
        "zh-CN",
        "en-US",
        "fr",
        "zh-Hant-TW",
        "es-419",
        "sr-Latn-RS",
        "de-DE-1996",
        "sl-rozaj-biske",
        "en-US-u-ca-gregory",
        "en-x-project",
        "en-u-foo-ca-gregory",
        "en-t-zh-hant",
        "hi-Latn-t-en-h0-hybrid",
    ] {
        assert!(
            validate_locales(locale, &[locale.into()]).is_ok(),
            "{locale}"
        );
    }
    for locale in [
        "",
        "en_US",
        "../en",
        "en/US",
        " en-US",
        "en-US ",
        "中文",
        "e-US",
        "abcd",
        "en--US",
        "en-u",
        "en-x",
        "en-1234-1234",
        "en-u-ca-u-nu",
        "en-US-verylongtag",
        "en-@private",
        "x-private",
        "en-u-12",
        "en-u-ca-gregory-ca-buddhist",
        "en-t-en-a1",
        "en-t-12-abc",
        "en-t-h0-hybrid-h0-other",
    ] {
        assert!(
            validate_locales(locale, &[locale.into()]).is_err(),
            "{locale}"
        );
    }
    assert!(validate_locales("zh-CN", &[]).is_err());
    assert!(validate_locales("zh-CN", &["en-US".into()]).is_err());
    assert!(validate_locales("zh-CN", &["zh-CN".into(), "zh-cn".into()]).is_err());
    assert!(validate_locales("zh-cn", &["zh-CN".into()]).is_err());
}

fn application() -> Result<ApplicationIR, serde_json::Error> {
    let source = json!({
        "path": "src/app.tsx",
        "span": {"start": 0, "end": 1,
            "start_position": {"line": 1, "column": 1},
            "end_position": {"line": 1, "column": 2}}
    });
    serde_json::from_value(json!({
        "format_version": 1, "name": "业务应用",
        "resources": [{"name": "customers", "source": source, "fields": [{
            "name": "status", "label": "状态", "kind": "select", "required": false,
            "default": "业务默认值", "integer": false, "min": null, "max": null,
            "max_length": null, "unique": false, "index": false,
            "options": [{"value": "pending", "label": "待处理", "when": {"region": "业务过滤值"}}],
            "options_source": {"resource": "regions", "value": "id", "label": "name",
                "filters": {"enabled": "flag"}, "permission": "anonymous"},
            "source": source
        }]}],
        "pages": [{"path": "/customers", "title": "客户页面", "permission": "anonymous",
            "source": source, "components": [{"type": "group", "label": "客户分组", "source": source,
                "children": [
                    {"type": "form", "id": "customer-form", "resource": "customers", "mode": "create",
                        "fields": ["status"], "labels": {"status": "表单状态"}, "source": source},
                    {"type": "table", "resource": "customers", "columns": ["status"], "source": source},
                    {"type": "text", "value": "欢迎登记", "source": source},
                    {"type": "image", "src": "/private-image.png", "alt": "客户图标",
                        "width": 20, "height": 20, "fit": "contain", "source": source},
                    {"type": "custom", "module": "Widget", "props": {
                        "url": {"kind": "literal", "value": "https://example.invalid/private"},
                        "data": {"kind": "literal", "value": "自定义业务数据"}},
                        "children": [{"type": "text", "value": "子节点文案", "source": source}], "source": source}
                ]}]}],
        "navigation": [{"label": "导航", "source": source, "items": [{"type": "group",
            "label": "导航分组", "source": source, "items": [
                {"type": "link", "label": "客户入口", "path": "/customers", "permission": "anonymous", "source": source},
                {"type": "link", "label": "欢迎登记", "path": "/welcome", "permission": "anonymous", "source": source}
            ]}]}],
        "actions": [], "permissions": [], "module_references": []
    }))
}

#[test]
fn defaults_cover_every_known_display_node_without_collecting_business_values() -> TestResult {
    let application = application()?;
    let catalog = defaults(&application);
    let expected = BTreeMap::from([
        ("app.name", "业务应用"),
        ("resources.customers.label", "customers"),
        ("resources.customers.fields.status.label", "状态"),
        (
            "resources.customers.fields.status.options.pending.label",
            "待处理",
        ),
        ("pages./customers.title", "客户页面"),
        ("text:客户分组", "客户分组"),
        ("text:表单状态", "表单状态"),
        ("text:欢迎登记", "欢迎登记"),
        ("text:客户图标", "客户图标"),
        ("text:子节点文案", "子节点文案"),
        ("text:导航", "导航"),
        ("text:导航分组", "导航分组"),
        ("text:客户入口", "客户入口"),
    ]);
    assert_eq!(catalog.len(), expected.len());
    for (key, value) in expected {
        assert_eq!(catalog.get(key).map(String::as_str), Some(value), "{key}");
    }
    for business_value in [
        "业务默认值",
        "业务过滤值",
        "自定义业务数据",
        "https://example.invalid/private",
        "/private-image.png",
    ] {
        assert!(!catalog.values().any(|value| value == business_value));
    }
    Ok(())
}

#[test]
fn text_keys_remain_stable_after_component_and_navigation_reordering() -> TestResult {
    let mut application = application()?;
    let before = defaults(&application);
    if let ComponentDefinition::Group { children, .. } = &mut application.pages[0].components[0] {
        children.reverse();
    }
    application.navigation.reverse();
    assert_eq!(defaults(&application), before);
    Ok(())
}
