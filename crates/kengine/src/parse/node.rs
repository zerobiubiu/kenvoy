//! XML 元素 → [`crate::ir::Node`] 组件树映射与属性解析辅助。

use std::collections::BTreeMap;

use serde_json::Value;

use crate::ir::{
    ColProps, ColumnProps, FieldProps, FormProps, Node, RowProps, SectionProps, StatCardProps,
    TableProps, TextProps, Validation,
};
use crate::parse::ParseError;

/// 将元素的子元素解析为组件树（跳过空白文本与注释）。
pub(crate) fn parse_children(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
) -> Result<Vec<Node>, ParseError> {
    let mut nodes = Vec::new();
    for child in elem.children().filter(roxmltree::Node::is_element) {
        // `<Validation>` 是表单元数据，不参与组件树
        if child.tag_name().name() == "Validation" {
            continue;
        }
        nodes.push(parse_node(path, child)?);
    }
    Ok(nodes)
}

/// 解析 `<Validation>` 下的 `<Rule>` 列表（无则返回空）。
pub(crate) fn parse_validations(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
) -> Result<Vec<Validation>, ParseError> {
    let Some(validation_elem) = find_child(elem, "Validation") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for rule in validation_elem
        .children()
        .filter(roxmltree::Node::is_element)
    {
        if rule.tag_name().name() != "Rule" {
            continue;
        }
        let field = required_attr(path, rule, "Rule", "field")?;
        let expr = required_attr(path, rule, "Rule", "expr")?;
        let message = text_content(rule).unwrap_or_else(|| "校验失败".to_string());
        out.push(Validation {
            field,
            expr,
            message,
        });
    }
    Ok(out)
}

/// 将单个元素解析为 [`Node`]；未知元素映射为 [`Node::Custom`]（用户 DSL 组件）。
pub(crate) fn parse_node(path: &str, elem: roxmltree::Node<'_, '_>) -> Result<Node, ParseError> {
    let tag = elem.tag_name().name();
    let node = match tag {
        "Row" => Node::Row {
            props: RowProps::default(),
            children: parse_children(path, elem)?,
        },
        "Col" => Node::Col {
            props: ColProps {
                span: parse_u8(path, elem, "span")?.unwrap_or(12),
            },
            children: parse_children(path, elem)?,
        },
        "Section" => Node::Section {
            props: SectionProps {
                title: elem.attribute("title").map(str::to_string),
            },
            children: parse_children(path, elem)?,
        },
        "Slot" => Node::Slot,
        "Form" => Node::Form {
            props: FormProps {
                model: required_attr(path, elem, tag, "model")?,
                validations: parse_validations(path, elem)?,
            },
            children: parse_children(path, elem)?,
        },
        "Field" => Node::Field {
            props: FieldProps {
                name: required_attr(path, elem, tag, "name")?,
                label: elem.attribute("label").map(str::to_string),
                readonly: parse_bool(path, elem, "readonly")?.unwrap_or(false),
                required: parse_bool(path, elem, "required")?.unwrap_or(false),
                default: elem.attribute("default").map(str::to_string),
                suffix: elem.attribute("suffix").map(str::to_string),
                rows: parse_u32(path, elem, "rows")?,
                options: split_options(elem.attribute("options")),
                validations: Vec::new(),
            },
        },
        "Table" => Node::Table {
            props: TableProps {
                model: required_attr(path, elem, tag, "model")?,
                filter: serde_json::Map::new(),
                sort: serde_json::Map::new(),
                page_size: parse_u32(path, elem, "pageSize")?,
            },
            children: parse_children(path, elem)?,
        },
        "Column" => Node::Column {
            props: ColumnProps {
                field: required_attr(path, elem, tag, "field")?,
                label: elem.attribute("label").map(str::to_string),
                width: parse_u16(path, elem, "width")?,
            },
        },
        "StatCard" => Node::StatCard {
            props: StatCardProps {
                data: elem.attribute("data").map(str::to_string),
                unit: elem.attribute("unit").map(str::to_string),
                color: elem.attribute("color").map(str::to_string),
            },
        },
        "Text" => Node::Text {
            props: TextProps {
                value: elem
                    .attribute("value")
                    .map(str::to_string)
                    .or_else(|| text_content(elem))
                    .ok_or_else(|| ParseError::missing_attribute(path, tag, "value"))?,
            },
        },
        // 未知元素 → 用户 DSL 自定义组件：组件名 = 标签名，属性原样透传
        _ => Node::Custom {
            component: tag.to_string(),
            props: elem
                .attributes()
                .map(|a| (a.name().to_string(), Value::String(a.value().to_string())))
                .collect::<BTreeMap<_, _>>(),
            children: parse_children(path, elem)?,
        },
    };
    Ok(node)
}

/// 多个节点折叠为单个根节点：0 个 → 空 `Row`，1 个 → 该节点，多个 → `Row` 包裹。
pub(crate) fn as_single_or_row(nodes: Vec<Node>) -> Node {
    if nodes.len() <= 1 {
        nodes.into_iter().next().unwrap_or_else(|| Node::Row {
            props: RowProps::default(),
            children: Vec::new(),
        })
    } else {
        Node::Row {
            props: RowProps::default(),
            children: nodes,
        }
    }
}

/// 查找指定标签的直接子元素。
pub(crate) fn find_child<'a, 'input>(
    elem: roxmltree::Node<'a, 'input>,
    tag: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    elem.children()
        .find(|n| n.is_element() && n.tag_name().name() == tag)
}

/// 读取指定子元素的文本内容。
pub(crate) fn child_text(elem: roxmltree::Node<'_, '_>, tag: &str) -> Option<String> {
    find_child(elem, tag).and_then(text_content)
}

/// 拼接元素文本子节点并去除首尾空白；为空返回 `None`。
pub(crate) fn text_content(elem: roxmltree::Node<'_, '_>) -> Option<String> {
    let mut s = String::new();
    for child in elem.children() {
        if child.is_text() {
            s.push_str(child.text().unwrap_or_default());
        }
    }
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// 读取必需属性。
pub(crate) fn required_attr(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    element: &str,
    attr: &str,
) -> Result<String, ParseError> {
    elem.attribute(attr)
        .map(str::to_string)
        .ok_or_else(|| ParseError::missing_attribute(path, element, attr))
}

/// 解析布尔属性（`true` / `false`），缺省为 `None`。
pub(crate) fn parse_bool(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    attr: &str,
) -> Result<Option<bool>, ParseError> {
    match elem.attribute(attr) {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(v) => Err(ParseError::invalid_attribute(path, attr, v)),
    }
}

/// 解析 `u8` 属性，缺省为 `None`。
pub(crate) fn parse_u8(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    attr: &str,
) -> Result<Option<u8>, ParseError> {
    parse_num(path, elem, attr)
}

/// 解析 `u16` 属性，缺省为 `None`。
pub(crate) fn parse_u16(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    attr: &str,
) -> Result<Option<u16>, ParseError> {
    parse_num(path, elem, attr)
}

/// 解析 `u32` 属性，缺省为 `None`。
pub(crate) fn parse_u32(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    attr: &str,
) -> Result<Option<u32>, ParseError> {
    parse_num(path, elem, attr)
}

fn parse_num<T>(
    path: &str,
    elem: roxmltree::Node<'_, '_>,
    attr: &str,
) -> Result<Option<T>, ParseError>
where
    T: std::str::FromStr,
{
    match elem.attribute(attr) {
        None => Ok(None),
        Some(v) => v
            .parse::<T>()
            .map(Some)
            .map_err(|_| ParseError::invalid_attribute(path, attr, v)),
    }
}

/// 按逗号拆分选项属性（去空白、去空项）。
pub(crate) fn split_options(s: Option<&str>) -> Vec<String> {
    match s {
        None => Vec::new(),
        Some(s) => s
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect(),
    }
}
