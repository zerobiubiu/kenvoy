//! 文档级解析：`<Models>` / `<Layout>` / `<Page>` / `<Flow>` / `<Publish>` → `IR` 实体。

use std::collections::BTreeMap;

use crate::ir::{
    DataFilterRule, EqCondition, FieldAccess, FieldDef, FieldType, Flow, FlowAction,
    FlowActionKind, FlowNode, FlowNodeKind, Layout, Model, Page, Publish, Role, RoleFilter,
};
use crate::parse::node::{
    as_single_or_row, child_text, find_child, parse_bool, parse_children, required_attr,
    split_options, text_content,
};
use crate::parse::{ParseError, SourceFile, resolve_import_path};

/// 解析 `<Models>` 文档（也兼容单个 `<Model>` 根元素）。
pub(crate) fn parse_models(
    path: &str,
    root: roxmltree::Node<'_, '_>,
) -> Result<Vec<Model>, ParseError> {
    if root.tag_name().name() == "Model" {
        return Ok(vec![parse_model(path, root)?]);
    }
    let mut models = Vec::new();
    for child in root.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() == "Model" {
            models.push(parse_model(path, child)?);
        }
        // 其他元素忽略（前向兼容）
    }
    Ok(models)
}

fn parse_model(path: &str, elem: roxmltree::Node<'_, '_>) -> Result<Model, ParseError> {
    let name = required_attr(path, elem, "Model", "name")?;
    let mut fields = Vec::new();
    for child in elem.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() == "Field" {
            fields.push(parse_field_def(path, child)?);
        }
    }
    Ok(Model { name, fields })
}

fn parse_field_def(path: &str, elem: roxmltree::Node<'_, '_>) -> Result<FieldDef, ParseError> {
    let name = required_attr(path, elem, "Field", "name")?;
    let type_raw = required_attr(path, elem, "Field", "type")?;
    let field_type = parse_field_type(&type_raw).ok_or_else(|| ParseError::InvalidFieldType {
        path: path.to_string(),
        value: type_raw,
    })?;
    Ok(FieldDef {
        name,
        field_type,
        label: elem.attribute("label").map(str::to_string),
        options: split_options(elem.attribute("options")),
        required: parse_bool(path, elem, "required")?.unwrap_or(false),
    })
}

fn parse_field_type(s: &str) -> Option<FieldType> {
    Some(match s {
        "text" => FieldType::Text,
        "textarea" => FieldType::Textarea,
        "number" => FieldType::Number,
        "date" => FieldType::Date,
        "select" => FieldType::Select,
        "boolean" => FieldType::Boolean,
        "file" => FieldType::File,
        "user" => FieldType::User,
        "department" => FieldType::Department,
        _ => return None,
    })
}

/// 解析 `<Layout>` 文档。
pub(crate) fn parse_layout(
    path: &str,
    root: roxmltree::Node<'_, '_>,
) -> Result<Layout, ParseError> {
    let root_elem = find_child(root, "Root").ok_or_else(|| ParseError::MissingElement {
        path: path.to_string(),
        element: "Root".to_string(),
    })?;
    let nodes = parse_children(path, root_elem)?;
    Ok(Layout {
        path: path.to_string(),
        title: child_text(root, "Title"),
        root: as_single_or_row(nodes),
    })
}

/// 解析 `<Page>` 文档（含 `<Import>` 引入的 flow / publish）。
pub(crate) fn parse_page(
    path: &str,
    root: roxmltree::Node<'_, '_>,
    files: &[SourceFile],
    flow_registry: &mut BTreeMap<String, Flow>,
) -> Result<Page, ParseError> {
    let content_elem = find_child(root, "Content").ok_or_else(|| ParseError::MissingElement {
        path: path.to_string(),
        element: "Content".to_string(),
    })?;
    let nodes = parse_children(path, content_elem)?;

    let mut flows = Vec::new();
    let mut publish = None;
    for child in root.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() != "Import" {
            continue;
        }
        let src = required_attr(path, child, "Import", "src")?;
        let import_path =
            resolve_import_path(path, &src).ok_or_else(|| ParseError::ImportInvalid {
                from: path.to_string(),
                src: src.clone(),
            })?;
        let imported = files
            .iter()
            .find(|f| f.path == import_path)
            .ok_or_else(|| ParseError::ImportNotFound {
                from: path.to_string(),
                src: src.clone(),
            })?;
        let doc =
            roxmltree::Document::parse(&imported.content).map_err(|source| ParseError::Xml {
                path: imported.path.clone(),
                source,
            })?;
        let root_elem = doc.root_element();
        match root_elem.tag_name().name() {
            "Flow" => {
                let flow = parse_flow(&imported.path, root_elem)?;
                if !flow_registry.contains_key(&flow.name) {
                    flow_registry.insert(flow.name.clone(), flow.clone());
                }
                flows.push(flow.name);
            }
            "Publish" => {
                publish = Some(parse_publish(&imported.path, root_elem)?);
            }
            other => {
                return Err(ParseError::UnknownImportType {
                    from: path.to_string(),
                    src: src.clone(),
                    root: other.to_string(),
                });
            }
        }
    }

    Ok(Page {
        path: path.to_string(),
        title: child_text(root, "Title"),
        layout_chain: Vec::new(),
        content: as_single_or_row(nodes),
        flows,
        publish,
    })
}

/// 解析 `<Flow>` 文档。
pub(crate) fn parse_flow(path: &str, root: roxmltree::Node<'_, '_>) -> Result<Flow, ParseError> {
    let name = required_attr(path, root, "Flow", "name")?;
    let trigger = root.attribute("trigger").map(str::to_string);
    let mut nodes = Vec::new();
    for child in root.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() == "Node" {
            nodes.push(parse_flow_node(path, child)?);
        }
    }
    Ok(Flow {
        name,
        trigger,
        nodes,
    })
}

fn parse_flow_node(path: &str, elem: roxmltree::Node<'_, '_>) -> Result<FlowNode, ParseError> {
    let name = required_attr(path, elem, "Node", "name")?;
    let approver = elem.attribute("approver").map(str::to_string);
    let kind = match elem.attribute("type") {
        Some("notify") => FlowNodeKind::Notify,
        Some("normal") | None => FlowNodeKind::Normal,
        Some(v) => {
            return Err(ParseError::InvalidAttribute {
                path: path.to_string(),
                attr: "type".to_string(),
                value: v.to_string(),
            });
        }
    };
    Ok(FlowNode {
        name,
        approver,
        kind,
        on_approve: parse_actions(path, find_child(elem, "OnApprove"))?,
        on_reject: parse_actions(path, find_child(elem, "OnReject"))?,
        on_complete: parse_actions(path, find_child(elem, "OnComplete"))?,
    })
}

/// 解析分支下的动作列表（未知元素忽略）。
fn parse_actions(
    path: &str,
    parent: Option<roxmltree::Node<'_, '_>>,
) -> Result<Vec<FlowAction>, ParseError> {
    let Some(parent) = parent else {
        return Ok(Vec::new());
    };
    let mut actions = Vec::new();
    for child in parent.children().filter(roxmltree::Node::is_element) {
        if let Some(action) = parse_action(path, child)? {
            actions.push(action);
        }
    }
    Ok(actions)
}

/// 解析单个流程动作；未知元素返回 `Ok(None)`（忽略）。
fn parse_action(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
) -> Result<Option<FlowAction>, ParseError> {
    let tag = elem.tag_name().name();
    match tag {
        "Action" => {
            let type_raw = required_attr(path, elem, "Action", "type")?;
            let mut action = match type_raw.as_str() {
                "next" => FlowAction::new(FlowActionKind::Next),
                "back" => FlowAction::new(FlowActionKind::Back),
                other => {
                    return Err(ParseError::InvalidAttribute {
                        path: path.to_string(),
                        attr: "type".to_string(),
                        value: other.to_string(),
                    });
                }
            };
            action.target = elem.attribute("target").map(str::to_string);
            Ok(Some(action))
        }
        "Notify" => {
            let mut action = FlowAction::new(FlowActionKind::Notify);
            action.users = split_options(elem.attribute("users"));
            Ok(Some(action))
        }
        "Update" => {
            let mut action = FlowAction::new(FlowActionKind::Update);
            action.model = elem.attribute("model").map(str::to_string);
            action.field = elem.attribute("field").map(str::to_string);
            action.value = elem.attribute("value").map(str::to_string);
            Ok(Some(action))
        }
        "SendMessage" => {
            let mut action = FlowAction::new(FlowActionKind::SendMessage);
            action.to = elem.attribute("to").map(str::to_string);
            action.value = text_content(elem);
            Ok(Some(action))
        }
        "If" => {
            let mut action = FlowAction::new(FlowActionKind::If);
            action.expr = elem.attribute("expr").map(str::to_string);
            action.children = parse_actions(path, Some(elem))?;
            Ok(Some(action))
        }
        _ => Ok(None),
    }
}

/// 解析 `<Publish>` 文档。
pub(crate) fn parse_publish(
    path: &str,
    root: roxmltree::Node<'_, '_>,
) -> Result<Publish, ParseError> {
    let mut allow_roles = Vec::new();
    let mut fields = Vec::new();
    let mut roles = Vec::new();
    let mut data_filters = Vec::new();
    for child in root.children().filter(roxmltree::Node::is_element) {
        match child.tag_name().name() {
            "Roles" => {
                for role_elem in child.children().filter(roxmltree::Node::is_element) {
                    if role_elem.tag_name().name() == "Role" {
                        roles.push(parse_role(path, role_elem)?);
                    }
                }
            }
            "Access" => {
                for access_child in child.children().filter(roxmltree::Node::is_element) {
                    match access_child.tag_name().name() {
                        "Allow" => {
                            allow_roles.extend(split_options(access_child.attribute("roles")));
                        }
                        "Field" => {
                            fields.push(parse_field_access(path, access_child)?);
                        }
                        _ => {}
                    }
                }
            }
            "DataFilter" => {
                data_filters.extend(parse_data_filters(path, child)?);
            }
            _ => {}
        }
    }
    Ok(Publish {
        allow_roles,
        fields,
        roles,
        data_filters,
    })
}

/// 解析 `<DataFilter>` 下的 `<Rule>`（模型级行过滤）。
fn parse_data_filters(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
) -> Result<Vec<DataFilterRule>, ParseError> {
    let mut filter_rules = Vec::new();
    for rule_elem in elem.children().filter(roxmltree::Node::is_element) {
        if rule_elem.tag_name().name() != "Rule" {
            continue;
        }
        let model = required_attr(path, rule_elem, "Rule", "model")?;
        let mut filters = Vec::new();
        for filter_elem in rule_elem.children().filter(roxmltree::Node::is_element) {
            if filter_elem.tag_name().name() != "Filter" {
                continue;
            }
            let role_names = split_options(filter_elem.attribute("role"));
            let mut all = false;
            let mut conditions = Vec::new();
            for cond in filter_elem.children().filter(roxmltree::Node::is_element) {
                match cond.tag_name().name() {
                    "All" => all = true,
                    "Eq" => conditions.push(EqCondition {
                        field: required_attr(path, cond, "Eq", "field")?,
                        value: required_attr(path, cond, "Eq", "value")?,
                    }),
                    _ => {}
                }
            }
            filters.push(RoleFilter {
                roles: role_names,
                all,
                conditions,
            });
        }
        filter_rules.push(DataFilterRule { model, filters });
    }
    Ok(filter_rules)
}

fn parse_role(path: &str, elem: roxmltree::Node<'_, '_>) -> Result<Role, ParseError> {
    let name = required_attr(path, elem, "Role", "name")?;
    let extends = elem.attribute("extends").map(str::to_string);
    let mut permissions = Vec::new();
    if let Some(perms) = find_child(elem, "Permissions") {
        for p in perms.children().filter(roxmltree::Node::is_element) {
            if p.tag_name().name() == "Permission"
                && let Some(text) = text_content(p)
            {
                permissions.push(text);
            }
        }
    }
    Ok(Role {
        name,
        extends,
        permissions,
    })
}

fn parse_field_access(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
) -> Result<FieldAccess, ParseError> {
    let field = required_attr(path, elem, "Field", "name")?;
    let mut read_roles = Vec::new();
    let mut write_roles = Vec::new();
    for child in elem.children().filter(roxmltree::Node::is_element) {
        match child.tag_name().name() {
            "Read" => read_roles.extend(split_options(child.attribute("roles"))),
            "Write" => write_roles.extend(split_options(child.attribute("roles"))),
            _ => {}
        }
    }
    Ok(FieldAccess {
        field,
        read_roles,
        write_roles,
    })
}
