use std::collections::BTreeMap;

use crate::domain::{
    model::{ComponentDefinition, FormMode, ModuleReferenceKind, PropValue},
    validation,
};

use super::super::{
    client_slice::ClientCodeTransformer,
    static_eval::{StaticNode, StaticValue},
};
use super::{
    Analyzer, Result,
    attributes::{attrs, error, list, no_children, positive_integer, required_string, string},
    fields::field_kind,
    pages::{PageState, check_projection},
};

impl<T: ClientCodeTransformer> Analyzer<'_, T> {
    pub(super) fn components(
        &mut self,
        nodes: &[StaticNode],
        state: &mut PageState<'_>,
    ) -> Result<Vec<ComponentDefinition>> {
        let mut output = vec![];
        for node in nodes {
            let element = match node {
                StaticNode::Text { value, source } => {
                    if !value.trim().is_empty() {
                        output.push(ComponentDefinition::Text {
                            value: value.clone(),
                            source: source.clone(),
                        });
                    }
                    continue;
                }
                StaticNode::Element(element) => element,
            };
            if field_kind(&element.name).is_some() && element.module_ref.is_none() {
                if !state.inserted_form {
                    let resource = state.auto_resource.ok_or_else(|| {
                        error(
                            element,
                            "KV2001",
                            "自定义组件内部不能声明数据库字段",
                            "将业务字段放在 Resource 或页面字段组",
                        )
                    })?;
                    state.inserted_form = true;
                    output.push(
                        self.make_form(
                            element,
                            resource.into(),
                            FormMode::Create,
                            state
                                .auto_fields
                                .iter()
                                .map(|field| field.name.clone())
                                .collect(),
                            state,
                        ),
                    );
                }
                continue;
            }
            let component = match element.name.as_str() {
                "form" if element.module_ref.is_none() => {
                    attrs(element, &["resource", "mode", "fields"])?;
                    no_children(element)?;
                    let resource = required_string(element, "resource")?;
                    let definition = self.resources.get(&resource).ok_or_else(|| {
                        error(
                            element,
                            "KV2011",
                            format!("未知 Resource {resource}"),
                            "先声明 Resource 或由其他页面明确推导",
                        )
                    })?;
                    let mode = match string(element, "mode")?.as_deref() {
                        None | Some("create") => FormMode::Create,
                        Some("edit") => FormMode::Edit,
                        _ => {
                            return Err(error(
                                element,
                                "KV2004",
                                "form.mode 只支持 create 或 edit",
                                "选择明确的表单操作",
                            ));
                        }
                    };
                    let fields = list(element, "fields")?.unwrap_or_else(|| {
                        definition
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect()
                    });
                    check_projection(element, definition, &fields)?;
                    if definition
                        .fields
                        .iter()
                        .filter(|field| fields.contains(&field.name))
                        .any(|field| {
                            field
                                .option_dependencies()
                                .iter()
                                .any(|dependency| !fields.iter().any(|name| name == dependency))
                        })
                    {
                        return Err(error(
                            element,
                            "KV2015",
                            "表单 fields 必须包含选项联动所需字段",
                            "将联动父字段加入同一表单，不能跨表单拼接权限",
                        ));
                    }
                    if mode == FormMode::Create
                        && definition.fields.iter().any(|field| {
                            field.required
                                && field.default.is_none()
                                && !fields.contains(&field.name)
                        })
                    {
                        return Err(error(
                            element,
                            "KV2007",
                            "创建表单遗漏没有默认值的必填字段",
                            "将必填字段加入 fields 或提供有效默认值",
                        ));
                    }
                    self.make_form(element, resource, mode, fields, state)
                }
                "table" if element.module_ref.is_none() => {
                    attrs(element, &["resource", "columns"])?;
                    no_children(element)?;
                    let resource = required_string(element, "resource")?;
                    let definition = self.resources.get(&resource).ok_or_else(|| {
                        error(
                            element,
                            "KV2011",
                            format!("未知 Resource {resource}"),
                            "先声明 Resource",
                        )
                    })?;
                    let columns = list(element, "columns")?.unwrap_or_else(|| {
                        definition
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect()
                    });
                    check_projection(element, definition, &columns)?;
                    ComponentDefinition::Table {
                        resource,
                        columns,
                        source: element.source.clone(),
                    }
                }
                "group" if element.module_ref.is_none() => {
                    attrs(element, &["label"])?;
                    ComponentDefinition::Group {
                        label: string(element, "label")?,
                        children: self.components(&element.children, state)?,
                        source: element.source.clone(),
                    }
                }
                "image" if element.module_ref.is_none() => {
                    attrs(element, &["src", "alt", "width", "height", "fit"])?;
                    no_children(element)?;
                    let src = required_string(element, "src")?;
                    if !validation::valid_image_src(&src) {
                        return Err(error(
                            element,
                            "KV2007",
                            "image.src 必须为无凭据的 HTTP(S) 绝对 URL 或单斜杠开头的站内路径",
                            "不能使用 //、反斜杠、控制字符或 javascript/data/blob URL",
                        ));
                    }
                    let alt = string(element, "alt")?.ok_or_else(|| {
                        error(
                            element,
                            "KV2004",
                            "image 必须显式声明 alt，可使用空字符串",
                            "提供图片说明；装饰图使用 alt=''",
                        )
                    })?;
                    let fit = string(element, "fit")?.unwrap_or_else(|| "contain".into());
                    if !matches!(fit.as_str(), "contain" | "cover") {
                        return Err(error(
                            element,
                            "KV2007",
                            "image.fit 只允许 contain 或 cover",
                            "明确选择图片适配方式",
                        ));
                    }
                    ComponentDefinition::Image {
                        src,
                        alt,
                        width: positive_integer(element, "width", 4096)?.map(|value| value as u32),
                        height: positive_integer(element, "height", 4096)?
                            .map(|value| value as u32),
                        fit,
                        source: element.source.clone(),
                    }
                }
                _ => {
                    let reference = element.module_ref.as_ref().ok_or_else(|| {
                        error(
                            element,
                            "KV2001",
                            format!("页面中未知标签 {}", element.name),
                            "使用内置 form/table/group/image 或导入普通 UI 组件",
                        )
                    })?;
                    let module = self.register_module(
                        reference,
                        ModuleReferenceKind::ClientComponent,
                        &element.source,
                        None,
                    );
                    let mut props = BTreeMap::new();
                    for (name, value) in &element.attributes {
                        let prop = match value {
                            StaticValue::Json(value) => PropValue::Literal(value.clone()),
                            StaticValue::Reference(reference) => {
                                PropValue::Module(self.register_module(
                                    reference,
                                    ModuleReferenceKind::ClientFunction,
                                    &element.source,
                                    None,
                                ))
                            }
                            StaticValue::Expression(reference) => PropValue::Module(
                                self.register_expression(reference, &element.source),
                            ),
                        };
                        props.insert(name.clone(), prop);
                    }
                    // UI children 仍由页面组件协议处理，但不能隐式贡献 Resource 字段。
                    let mut child_state = PageState {
                        path: state.path,
                        permission: state.permission,
                        auto_resource: None,
                        auto_fields: &[],
                        inserted_form: false,
                        form_count: state.form_count,
                    };
                    let children = self.components(&element.children, &mut child_state)?;
                    state.form_count = child_state.form_count;
                    ComponentDefinition::Custom {
                        module,
                        props,
                        children,
                        source: element.source.clone(),
                    }
                }
            };
            output.push(component);
        }
        Ok(output)
    }
}
