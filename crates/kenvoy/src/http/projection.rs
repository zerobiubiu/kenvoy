//! 显式构造浏览器 DTO；宿主源码位置、模块路径和服务端实现从不序列化。

use super::permits;
use crate::application::{
    ApplicationIR, ComponentDefinition, ExecutionEnvironment, FieldDefinition, NavigationItem,
    PermissionTarget, Role,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn application(application: &ApplicationIR, role: Role, generation: u64) -> Value {
    let mut visible = BTreeMap::<String, BTreeSet<String>>::new();
    let pages: Vec<_> = application.pages.iter().filter(|page| permits(role, page.permission)).map(|page| {
        json!({"path":page.path,"title":page.title,"permission":page.permission,"components":page.components.iter().map(|node|component(node,&mut visible)).collect::<Vec<_>>()})
    }).collect();
    if role == Role::Admin {
        for resource in &application.resources {
            visible.insert(
                resource.name.clone(),
                resource
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
            );
        }
    }
    let resources: Vec<_> = application.resources.iter().filter_map(|resource| visible.get(&resource.name).map(|names| json!({"name":resource.name,"fields":resource.fields.iter().filter(|field|names.contains(&field.name)).map(field).collect::<Vec<_>>()}))).collect();
    let navigation: Vec<_> = application
        .navigation
        .iter()
        .filter_map(|nav| {
            let items: Vec<_> = nav
                .items
                .iter()
                .filter_map(|item| navigation_item(item, role))
                .collect();
            (!items.is_empty()).then(|| json!({"label":nav.label,"items":items}))
        })
        .collect();
    let actions: Vec<_> = application
        .actions
        .iter()
        .filter(|action| permits(role, action.permission))
        .map(|action| {
            let mut value =
                json!({"name":action.name,"runtime":action.runtime,"permission":action.permission});
            if action.runtime != ExecutionEnvironment::Server {
                value["handler"] = json!(action.handler);
            }
            value
        })
        .collect();
    let permissions: Vec<_> = application.permissions.iter().filter(|policy|permits(role,policy.role)).filter(|policy|match &policy.target {PermissionTarget::Resource(name)=>visible.contains_key(name),PermissionTarget::Action(_)=>true}).map(|policy|json!({"target":policy.target,"operation":policy.operation,"fields":policy.fields,"form":policy.form})).collect();
    json!({"format_version":1,"name":application.name,"generation":generation,"role":role,"pages":pages,"resources":resources,"navigation":navigation,"actions":actions,"permissions":permissions})
}

fn field(field: &FieldDefinition) -> Value {
    let mut value = json!({"name":field.name,"label":field.label,"kind":field.kind,"required":field.required,"integer":field.integer,"min":field.min,"max":field.max,"max_length":field.max_length,"unique":field.unique,"options":field.options});
    if let Some(default) = &field.default {
        value["default"] = default.clone();
    }
    value
}

fn component(
    node: &ComponentDefinition,
    visible: &mut BTreeMap<String, BTreeSet<String>>,
) -> Value {
    match node {
        ComponentDefinition::Form {
            id,
            resource,
            mode,
            fields,
            labels,
            ..
        } => {
            visible
                .entry(resource.clone())
                .or_default()
                .extend(fields.iter().cloned());
            json!({"type":"form","id":id,"resource":resource,"mode":mode,"fields":fields,"labels":labels})
        }
        ComponentDefinition::Table {
            resource, columns, ..
        } => {
            visible
                .entry(resource.clone())
                .or_default()
                .extend(columns.iter().cloned());
            json!({"type":"table","resource":resource,"columns":columns})
        }
        ComponentDefinition::Group {
            label, children, ..
        } => {
            json!({"type":"group","label":label,"children":children.iter().map(|node|component(node,visible)).collect::<Vec<_>>()})
        }
        ComponentDefinition::Custom {
            module,
            props,
            children,
            ..
        } => {
            json!({"type":"custom","module":module,"props":props,"children":children.iter().map(|node|component(node,visible)).collect::<Vec<_>>()})
        }
        ComponentDefinition::Text { value, .. } => json!({"type":"text","value":value}),
    }
}

fn navigation_item(item: &NavigationItem, role: Role) -> Option<Value> {
    match item {
        NavigationItem::Link {
            label,
            path,
            permission,
            ..
        } => permits(role, *permission).then(|| json!({"type":"link","label":label,"path":path})),
        NavigationItem::Group { label, items, .. } => {
            let items: Vec<_> = items
                .iter()
                .filter_map(|item| navigation_item(item, role))
                .collect();
            (!items.is_empty()).then(|| json!({"type":"group","label":label,"items":items}))
        }
    }
}
