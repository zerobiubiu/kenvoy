use crate::domain::model::{
    NavigationDefinition, NavigationItem, PermissionTarget, ResourceDefinition, Role,
};

use super::super::{
    client_slice::ClientCodeTransformer,
    static_eval::{StaticElement, StaticNode},
};
use super::{
    Analyzer, Result,
    attributes::{
        attrs, elements, error, identifier, intrinsic, no_children, required_string,
        semantic_error, source_of, string,
    },
    fields::{field, unique_fields},
};

impl<T: ClientCodeTransformer> Analyzer<'_, T> {
    pub(super) fn root(&mut self, nodes: &[StaticNode], default_name: &str) -> Result<String> {
        let roots = elements(nodes)?;
        if roots.len() != 1 {
            let source = roots
                .first()
                .map(|item| item.source.clone())
                .or_else(|| nodes.first().map(source_of))
                .unwrap_or_else(|| self.entry_source.clone());
            return Err(semantic_error(
                source,
                "KV2001",
                "默认导出必须恰好包含一个 app 或 page 根",
                "使用单个 app 包含所有声明",
            ));
        }
        let root = roots[0];
        intrinsic(root)?;
        let (name, children) = if root.name == "app" {
            attrs(root, &["name"])?;
            (required_string(root, "name")?, elements(&root.children)?)
        } else if root.name == "page" {
            (default_name.to_owned(), vec![root])
        } else {
            return Err(error(
                root,
                "KV2001",
                format!("不支持的应用根 {}", root.name),
                "入口默认导出 app 或 page 声明",
            ));
        };
        // 显式资源先登记，避免源码顺序改变推导规则。
        for child in &children {
            if child.name == "resource"
                && let Err(error) = self.explicit_resource(child)
            {
                self.diagnostics.push(error);
            }
        }
        let mut raw_pages = Vec::new();
        let mut raw_navigation = Vec::new();
        for child in children {
            match child.name.as_str() {
                "resource" => {}
                "page" => match self.prepare_page(child) {
                    Ok(page) => raw_pages.push(page),
                    Err(error) => self.diagnostics.push(error),
                },
                "action" => {
                    if let Err(error) = self.action_definition(child) {
                        self.diagnostics.push(error);
                    }
                }
                "navigation" => raw_navigation.push(child),
                "override" => {
                    if let Err(error) = self.override_definition(child) {
                        self.diagnostics.push(error);
                    }
                }
                _ => self.diagnostics.push(error(
                    child,
                    "KV2001",
                    format!("app 内不允许标签 {}", child.name),
                    "app 只包含 resource、page、action、navigation 或 override",
                )),
            }
        }
        // 所有显式/隐式资源存在后，再解析 form/table 的引用。
        for prepared in raw_pages {
            if let Err(error) = self.finish_page(prepared) {
                self.diagnostics.push(error);
            }
        }
        for navigation in raw_navigation {
            match self.navigation_definition(navigation) {
                Ok(definition) => self.navigation.push(definition),
                Err(error) => self.diagnostics.push(error),
            }
        }
        if let Err((source, message)) = crate::domain::options::validate_definitions(
            &self.resources.values().cloned().collect::<Vec<_>>(),
        ) {
            self.diagnostics.push(semantic_error(
                source,
                "KV2015",
                message,
                "检查选项来源、联动字段及依赖循环",
            ));
        }
        for policy in &self.permissions {
            let PermissionTarget::Resource(name) = &policy.target else {
                continue;
            };
            let Some(resource) = self.resources.get(name) else {
                continue;
            };
            for field in &resource.fields {
                if let Some(source) = &field.options_source
                    && (policy.fields.contains(&field.name)
                        || field.default.as_ref().is_some_and(|value| !value.is_null()))
                    && policy.role != Role::Admin
                    && source.permission != Role::Anonymous
                    && policy.role != source.permission
                {
                    self.diagnostics.push(semantic_error(
                        field.source.clone(),
                        "KV2015",
                        format!("表单权限不足以查询或提交字段 {} 的选项来源", field.name),
                        "提高页面权限、缩小表单字段范围，或明确降低来源投影权限",
                    ));
                }
            }
        }
        Ok(name)
    }

    fn explicit_resource(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name"])?;
        let name = required_string(element, "name")?;
        if !identifier(&name) {
            return Err(error(
                element,
                "KV2006",
                format!("资源名 {name} 非法或保留"),
                "使用 a-z 开头的小写字母、数字和下划线",
            ));
        }
        if let Some(previous) = self.resources.get(&name) {
            let mut failure = error(
                element,
                "KV2005",
                format!("重复 Resource {name}"),
                "同名显式 Resource 只能声明一次",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let fields = elements(&element.children)?
            .into_iter()
            .map(field)
            .collect::<Result<Vec<_>>>()?;
        unique_fields(&fields)?;
        if fields.is_empty() {
            return Err(error(
                element,
                "KV2007",
                "Resource 至少需要一个字段",
                "声明静态字段；不要用动态 JSX 生成 Schema",
            ));
        }
        self.resources.insert(
            name.clone(),
            ResourceDefinition {
                name,
                fields,
                source: element.source.clone(),
            },
        );
        Ok(())
    }

    fn navigation_definition(&self, element: &StaticElement) -> Result<NavigationDefinition> {
        intrinsic(element)?;
        attrs(element, &["label"])?;
        Ok(NavigationDefinition {
            label: string(element, "label")?,
            items: self.navigation_items(&element.children)?,
            source: element.source.clone(),
        })
    }

    fn navigation_items(&self, nodes: &[StaticNode]) -> Result<Vec<NavigationItem>> {
        elements(nodes)?
            .into_iter()
            .map(|element| {
                intrinsic(element)?;
                match element.name.as_str() {
                    "item" => {
                        attrs(element, &["label", "path"])?;
                        no_children(element)?;
                        let path = required_string(element, "path")?;
                        let page = self
                            .pages
                            .iter()
                            .find(|page| page.path == path)
                            .ok_or_else(|| {
                                error(
                                    element,
                                    "KV2011",
                                    format!("导航目标 {path} 不存在"),
                                    "导航只引用已声明页面",
                                )
                            })?;
                        Ok(NavigationItem::Link {
                            label: required_string(element, "label")?,
                            path,
                            permission: page.permission,
                            source: element.source.clone(),
                        })
                    }
                    "group" => {
                        attrs(element, &["label"])?;
                        Ok(NavigationItem::Group {
                            label: string(element, "label")?,
                            items: self.navigation_items(&element.children)?,
                            source: element.source.clone(),
                        })
                    }
                    _ => Err(error(
                        element,
                        "KV2001",
                        "navigation 只允许 item 或 group",
                        "将页面布局组件移到 page",
                    )),
                }
            })
            .collect()
    }
}
