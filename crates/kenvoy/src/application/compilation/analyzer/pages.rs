use std::collections::BTreeMap;

use crate::domain::model::{
    ComponentDefinition, FieldDefinition, FormMode, Operation, PageDefinition,
    PermissionDefinition, PermissionTarget, ResourceDefinition, Role,
};

use super::super::{client_slice::ClientCodeTransformer, static_eval::StaticElement};
use super::{
    Analyzer, Result,
    attributes::{attrs, error, identifier, intrinsic, required_string, role, route, string},
    fields::{collect_page_fields, has_control, unique_fields},
};

pub(super) struct PreparedPage<'a> {
    element: &'a StaticElement,
    path: String,
    resource: Option<String>,
    fields: Vec<FieldDefinition>,
}

pub(super) struct PageState<'a> {
    pub(super) path: &'a str,
    pub(super) permission: Role,
    pub(super) auto_resource: Option<&'a str>,
    pub(super) auto_fields: &'a [FieldDefinition],
    pub(super) inserted_form: bool,
    pub(super) form_count: usize,
}

impl<T: ClientCodeTransformer> Analyzer<'_, T> {
    pub(super) fn prepare_page<'a>(
        &mut self,
        element: &'a StaticElement,
    ) -> Result<PreparedPage<'a>> {
        intrinsic(element)?;
        attrs(element, &["path", "title", "resource", "permission"])?;
        let path = required_string(element, "path")?;
        if !route(&path) {
            return Err(error(
                element,
                "KV2009",
                format!("页面路径 {path} 非法或占用保留命名空间"),
                "使用规范静态绝对路径，避开 /api、/admin 和 /_kenvoy",
            ));
        }
        let mut fields = vec![];
        collect_page_fields(&element.children, &mut fields)?;
        unique_fields(&fields)?;
        let resource = if fields.is_empty() {
            if element.attributes.contains_key("resource") {
                return Err(error(
                    element,
                    "KV2007",
                    "page.resource 仅用于页面字段简写",
                    "已有 Resource 请使用 form/table 的 resource 属性",
                ));
            }
            None
        } else {
            if has_control(&element.children) {
                return Err(error(
                    element,
                    "KV2007",
                    "页面字段简写不能与显式 form/table 混用",
                    "将字段移入显式 Resource 并使用 form/table",
                ));
            }
            let name = match string(element, "resource")? {
                Some(name) => name,
                None if path.matches('/').count() == 1 => path.trim_start_matches('/').into(),
                None => {
                    return Err(error(
                        element,
                        "KV2007",
                        "多段路径的字段简写必须明确 page.resource",
                        "为多个页面指定同一个稳定资源名，避免意外创建不同数据表",
                    ));
                }
            };
            if !identifier(&name) {
                return Err(error(
                    element,
                    "KV2006",
                    format!("无法使用资源名 {name}"),
                    "显式指定符合标识符规范的 page.resource",
                ));
            }
            if let Some(previous) = self.resources.get(&name) {
                if fields.len() != previous.fields.len()
                    || !fields.iter().all(|field| {
                        previous
                            .field(&field.name)
                            .is_some_and(|old| old.same_schema(field))
                    })
                {
                    let mut failure = error(
                        element,
                        "KV2010",
                        format!("页面字段与 Resource {name} 的完整结构不一致"),
                        "复用显式 form 或提供相同字段、类型、默认值和约束，不能静默合并",
                    );
                    failure.related.push(previous.source.clone());
                    return Err(failure);
                }
            } else {
                self.resources.insert(
                    name.clone(),
                    ResourceDefinition {
                        name: name.clone(),
                        fields: fields.clone(),
                        source: element.source.clone(),
                    },
                );
            }
            Some(name)
        };
        Ok(PreparedPage {
            element,
            path,
            resource,
            fields,
        })
    }

    pub(super) fn finish_page(&mut self, prepared: PreparedPage<'_>) -> Result<()> {
        if let Some(previous) = self.pages.iter().find(|page| page.path == prepared.path) {
            let mut failure = error(
                prepared.element,
                "KV2009",
                format!("重复页面路径 {}", prepared.path),
                "每个页面使用唯一规范路径",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let permission = role(prepared.element, false)?;
        let mut state = PageState {
            path: &prepared.path,
            permission,
            auto_resource: prepared.resource.as_deref(),
            auto_fields: &prepared.fields,
            inserted_form: false,
            form_count: 0,
        };
        let components = self.components(&prepared.element.children, &mut state)?;
        self.pages.push(PageDefinition {
            title: string(prepared.element, "title")?.unwrap_or_else(|| prepared.path.clone()),
            path: prepared.path,
            permission,
            components,
            source: prepared.element.source.clone(),
        });
        Ok(())
    }

    pub(super) fn make_form(
        &mut self,
        element: &StaticElement,
        resource: String,
        mode: FormMode,
        fields: Vec<String>,
        state: &mut PageState<'_>,
    ) -> ComponentDefinition {
        state.form_count += 1;
        let id = format!("{}#form-{}", state.path, state.form_count);
        if mode == FormMode::Create && state.permission != Role::Admin {
            self.permissions.push(PermissionDefinition {
                target: PermissionTarget::Resource(resource.clone()),
                operation: Operation::Create,
                role: state.permission,
                fields: fields.clone(),
                form: Some(id.clone()),
            });
        }
        let labels = if state.auto_resource == Some(resource.as_str()) {
            state
                .auto_fields
                .iter()
                .map(|field| (field.name.clone(), field.label.clone()))
                .collect()
        } else {
            BTreeMap::new()
        };
        ComponentDefinition::Form {
            id,
            resource,
            mode,
            fields,
            labels,
            source: element.source.clone(),
        }
    }
}

pub(super) fn check_projection(
    element: &StaticElement,
    resource: &ResourceDefinition,
    fields: &[String],
) -> Result<()> {
    if let Some(name) = fields.iter().find(|name| resource.field(name).is_none()) {
        return Err(error(
            element,
            "KV2011",
            format!("Resource {} 没有字段 {name}", resource.name),
            "投影只允许已声明的业务字段",
        ));
    }
    Ok(())
}
