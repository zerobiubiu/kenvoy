//! `IR` 组件树 → HTML 片段渲染。

use std::fmt::Write;

use kengine::ir::{
    FieldDef, FieldProps, FieldType, Model, Node, ProjectIr, StatCardProps, TableProps,
};

use crate::escape::escape_html;

/// 渲染节点列表。
pub(crate) fn render_nodes(ir: &ProjectIr, nodes: &[Node], slot: &str) -> String {
    nodes.iter().map(|n| render_node(ir, n, slot)).collect()
}

/// 渲染单个节点；`slot` 注入到 [`Node::Slot`] 位置。
pub(crate) fn render_node(ir: &ProjectIr, node: &Node, slot: &str) -> String {
    match node {
        Node::Row { children, .. } => {
            let body = render_nodes(ir, children, slot);
            format!("<div class=\"kv-row\">{body}</div>")
        }
        Node::Col { props, children } => {
            let body = render_nodes(ir, children, slot);
            let width = f64::from(props.span) * 100.0 / 12.0;
            format!("<div class=\"kv-col\" style=\"width:{width:.0}%\">{body}</div>")
        }
        Node::Section { props, children } => {
            let body = render_nodes(ir, children, slot);
            let title = props
                .title
                .as_deref()
                .map(|t| format!("<h3 class=\"kv-section-title\">{}</h3>", escape_html(t)))
                .unwrap_or_default();
            format!("<section class=\"kv-section\">{title}{body}</section>")
        }
        Node::Slot => slot.to_string(),
        Node::Form { props, children } => {
            let model = ir.models.get(&props.model);
            let model_name = escape_html(&props.model);
            let fields: String = children
                .iter()
                .map(|c| match c {
                    Node::Field { props } => render_field(model, props),
                    other => render_node(ir, other, slot),
                })
                .collect();
            format!("<form class=\"kv-form\" data-model=\"{model_name}\">{fields}</form>")
        }
        Node::Field { props } => render_field(None, props),
        Node::Table { props, children } => render_table(ir, props, children),
        Node::Column { props } => {
            let label = props.label.as_deref().unwrap_or(&props.field);
            format!(
                "<th class=\"kv-th\" data-field=\"{}\">{}</th>",
                escape_html(&props.field),
                escape_html(label)
            )
        }
        Node::StatCard { props } => render_stat_card(props),
        Node::Text { props } => {
            let body = escape_html(&props.value);
            format!("<span class=\"kv-text\">{body}</span>")
        }
        Node::Custom {
            component,
            children,
            ..
        } => {
            let body = render_nodes(ir, children, slot);
            format!(
                "<div class=\"kv-custom\" data-component=\"{}\">{body}</div>",
                escape_html(component)
            )
        }
    }
}

/// 渲染表单：按模型字段类型渲染控件。
fn render_field(model: Option<&Model>, props: &FieldProps) -> String {
    let field_def = model.and_then(|m| m.fields.iter().find(|f| f.name == props.name));
    let field_type = field_def.map_or(FieldType::Text, |f| f.field_type);
    let label = props
        .label
        .as_deref()
        .or_else(|| field_def.and_then(|f| f.label.as_deref()))
        .unwrap_or(&props.name);
    let label_html = escape_html(label);
    let name = escape_html(&props.name);
    let disabled = if props.readonly { " disabled" } else { "" };
    let required_attr = if props.required { " required" } else { "" };
    let required_mark = if props.required { " *" } else { "" };
    let value_attr = props
        .default
        .as_deref()
        .map(|v| format!(" value=\"{}\"", escape_html(v)))
        .unwrap_or_default();

    let control = match field_type {
        FieldType::Textarea => {
            let rows = props
                .rows
                .map(|r| format!(" rows=\"{r}\""))
                .unwrap_or_default();
            let body = escape_html(props.default.as_deref().unwrap_or(""));
            format!(
                "<textarea class=\"kv-control\" name=\"{name}\"{rows}{disabled}{required_attr}>{body}</textarea>"
            )
        }
        FieldType::Select => {
            let options = options_for(props, field_def);
            let mut options_html = String::new();
            for opt in options {
                let opt = escape_html(opt);
                let _ = write!(options_html, "<option value=\"{opt}\">{opt}</option>");
            }
            format!(
                "<select class=\"kv-control\" name=\"{name}\"{disabled}{required_attr}>{options_html}</select>"
            )
        }
        FieldType::Boolean => {
            format!("<input class=\"kv-control\" type=\"checkbox\" name=\"{name}\"{disabled} />")
        }
        FieldType::Number => {
            format!(
                "<input class=\"kv-control\" type=\"number\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
            )
        }
        FieldType::Date => {
            format!(
                "<input class=\"kv-control\" type=\"date\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
            )
        }
        FieldType::File => {
            format!(
                "<input class=\"kv-control\" type=\"file\" name=\"{name}\"{disabled}{required_attr} />"
            )
        }
        _ => format!(
            "<input class=\"kv-control\" type=\"text\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
        ),
    };

    let suffix = props
        .suffix
        .as_deref()
        .map(|s| format!("<span class=\"kv-suffix\">{}</span>", escape_html(s)))
        .unwrap_or_default();

    format!(
        "<div class=\"kv-field\"><label class=\"kv-label\">{label_html}{required_mark}</label>{control}{suffix}</div>"
    )
}

/// 字段选项：页面字段覆盖优先，否则用模型定义。
fn options_for<'a>(props: &'a FieldProps, field_def: Option<&'a FieldDef>) -> &'a [String] {
    if props.options.is_empty() {
        field_def.map(|f| f.options.as_slice()).unwrap_or_default()
    } else {
        props.options.as_slice()
    }
}

/// 渲染数据表格：表头 + 空态（demo 暂无真实数据）。
fn render_table(ir: &ProjectIr, props: &TableProps, children: &[Node]) -> String {
    let head: String = children.iter().map(|c| render_node(ir, c, "")).collect();
    let empty = if children.is_empty() {
        String::new()
    } else {
        format!(
            "<tr><td class=\"kv-empty\" colspan=\"{}\">暂无数据</td></tr>",
            children.len()
        )
    };
    let model_name = escape_html(&props.model);
    format!(
        "<div class=\"kv-table-wrap\" data-model=\"{model_name}\"><table class=\"kv-table\"><thead><tr>{head}</tr></thead><tbody>{empty}</tbody></table></div>"
    )
}

/// 渲染统计卡。
fn render_stat_card(props: &StatCardProps) -> String {
    let data = props.data.as_deref().map(escape_html).unwrap_or_default();
    let unit = props.unit.as_deref().map(escape_html).unwrap_or_default();
    let color = escape_html(props.color.as_deref().unwrap_or("blue"));
    let unit_html = if unit.is_empty() {
        String::new()
    } else {
        format!("<span class=\"kv-stat-unit\">{unit}</span>")
    };
    format!(
        "<div class=\"kv-stat kv-stat-{color}\"><span class=\"kv-stat-data\">{data}</span>{unit_html}</div>"
    )
}
