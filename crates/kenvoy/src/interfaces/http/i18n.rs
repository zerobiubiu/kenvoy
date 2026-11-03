//! 语言词典沿用应用描述的授权投影，不把隐藏页面或字段的原生词条发给浏览器。
use std::collections::BTreeSet;

use serde_json::Value;

use crate::domain::i18n::I18nBundle;

pub(super) fn project(bundle: &I18nBundle, application: &Value) -> I18nBundle {
    let mut allowed = BTreeSet::from(["app.name".to_owned()]);
    text(&application["name"], &mut allowed);
    if let Some(resources) = application["resources"].as_array() {
        for resource in resources {
            let Some(name) = resource["name"].as_str() else {
                continue;
            };
            allowed.insert(format!("resources.{name}.label"));
            text(&resource["name"], &mut allowed);
            if let Some(fields) = resource["fields"].as_array() {
                for field in fields {
                    let Some(field_name) = field["name"].as_str() else {
                        continue;
                    };
                    let key = format!("resources.{name}.fields.{field_name}");
                    allowed.insert(format!("{key}.label"));
                    text(&field["label"], &mut allowed);
                    if let Some(options) = field["options"].as_array() {
                        for option in options {
                            if let Some(value) = option["value"].as_str() {
                                allowed.insert(format!("{key}.options.{value}.label"));
                            }
                            text(&option["label"], &mut allowed);
                        }
                    }
                }
            }
        }
    }
    if let Some(pages) = application["pages"].as_array() {
        for page in pages {
            if let Some(path) = page["path"].as_str() {
                allowed.insert(format!("pages.{path}.title"));
            }
            text(&page["title"], &mut allowed);
            components(&page["components"], &mut allowed);
        }
    }
    if let Some(navigation) = application["navigation"].as_array() {
        for group in navigation {
            text(&group["label"], &mut allowed);
            navigation_items(&group["items"], &mut allowed);
        }
    }
    let messages = bundle
        .messages
        .iter()
        .map(|(locale, entries)| {
            let entries = entries
                .iter()
                .filter(|(key, _)| !native_key(key) || allowed.contains(*key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            (locale.clone(), entries)
        })
        .collect();
    I18nBundle {
        format_version: bundle.format_version,
        fallback: bundle.fallback.clone(),
        languages: bundle.languages.clone(),
        messages,
    }
}

fn native_key(key: &str) -> bool {
    key == "app.name"
        || key.starts_with("resources.")
        || key.starts_with("pages.")
        || key.starts_with("text:")
}

fn text(value: &Value, allowed: &mut BTreeSet<String>) {
    if let Some(value) = value.as_str() {
        allowed.insert(format!("text:{value}"));
    }
}

fn components(nodes: &Value, allowed: &mut BTreeSet<String>) {
    let Some(nodes) = nodes.as_array() else {
        return;
    };
    for node in nodes {
        match node["type"].as_str() {
            Some("text") => text(&node["value"], allowed),
            Some("image") => text(&node["alt"], allowed),
            Some("group") => text(&node["label"], allowed),
            Some("form") => {
                if let Some(labels) = node["labels"].as_object() {
                    for label in labels.values() {
                        text(label, allowed);
                    }
                }
            }
            _ => {}
        }
        // 自定义组件的任意 props 可能是 URL/业务值，不作为展示词条扫描。
        components(&node["children"], allowed);
    }
}

fn navigation_items(items: &Value, allowed: &mut BTreeSet<String>) {
    let Some(items) = items.as_array() else {
        return;
    };
    for item in items {
        text(&item["label"], allowed);
        navigation_items(&item["items"], allowed);
    }
}
