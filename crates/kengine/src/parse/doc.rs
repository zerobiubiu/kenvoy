//! 文档级解析：`<Models>` / `<Layout>` / `<Page>` → `IR` 实体。

use crate::ir::{FieldDef, FieldType, Layout, Model, Page};
use crate::parse::ParseError;
use crate::parse::node::{
    as_single_or_row, child_text, find_child, parse_bool, parse_children, required_attr,
    split_options,
};

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

/// 解析 `<Page>` 文档。
pub(crate) fn parse_page(path: &str, root: roxmltree::Node<'_, '_>) -> Result<Page, ParseError> {
    let content_elem = find_child(root, "Content").ok_or_else(|| ParseError::MissingElement {
        path: path.to_string(),
        element: "Content".to_string(),
    })?;
    let nodes = parse_children(path, content_elem)?;
    Ok(Page {
        path: path.to_string(),
        title: child_text(root, "Title"),
        layout_chain: Vec::new(),
        content: as_single_or_row(nodes),
    })
}
