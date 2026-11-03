use std::collections::BTreeMap;

use crate::domain::model::{ApplicationIR, ComponentDefinition, NavigationItem};

/// 原生键只对应展示文案；标识、默认值、过滤条件和自定义组件 props 不进入词典。
pub fn defaults(application: &ApplicationIR) -> BTreeMap<String, String> {
    let mut messages = BTreeMap::from([("app.name".into(), application.name.clone())]);
    for resource in &application.resources {
        let prefix = format!("resources.{}", resource.name);
        messages.insert(format!("{prefix}.label"), resource.name.clone());
        for field in &resource.fields {
            let prefix = format!("{prefix}.fields.{}", field.name);
            messages.insert(format!("{prefix}.label"), field.label.clone());
            for option in &field.options {
                messages.insert(
                    format!("{prefix}.options.{}.label", option.value),
                    option.label.clone(),
                );
            }
        }
    }
    for page in &application.pages {
        messages.insert(format!("pages.{}.title", page.path), page.title.clone());
        components(&page.components, &mut messages);
    }
    for navigation in &application.navigation {
        if let Some(label) = &navigation.label {
            text(label, &mut messages);
        }
        navigation_items(&navigation.items, &mut messages);
    }
    messages
}

fn text(value: &str, messages: &mut BTreeMap<String, String>) {
    if !value.trim().is_empty() {
        messages
            .entry(format!("text:{value}"))
            .or_insert_with(|| value.into());
    }
}

fn components(items: &[ComponentDefinition], messages: &mut BTreeMap<String, String>) {
    for component in items {
        match component {
            ComponentDefinition::Form { labels, .. } => {
                for label in labels.values() {
                    text(label, messages);
                }
            }
            ComponentDefinition::Group {
                label, children, ..
            } => {
                if let Some(label) = label {
                    text(label, messages);
                }
                components(children, messages);
            }
            ComponentDefinition::Custom { children, .. } => components(children, messages),
            ComponentDefinition::Text { value, .. } => text(value, messages),
            ComponentDefinition::Image { alt, .. } => text(alt, messages),
            ComponentDefinition::Table { .. } => {}
        }
    }
}

fn navigation_items(items: &[NavigationItem], messages: &mut BTreeMap<String, String>) {
    for item in items {
        match item {
            NavigationItem::Link { label, .. } => text(label, messages),
            NavigationItem::Group { label, items, .. } => {
                if let Some(label) = label {
                    text(label, messages);
                }
                navigation_items(items, messages);
            }
        }
    }
}
