//! `IR` 组件树 → HTML 片段渲染。
//!
//! 样式基于 [Tabler](https://github.com/tabler/tabler)（Bootstrap 5 风格）：
//! 输出 Tabler/Bootstrap 类名（`row` / `col-md-*` / `card` / `form-control` / `table` 等），
//! 保留 `kv-*` 语义类与 `data-*` 属性供客户端运行时与扩展使用。
//!
//! 自定义组件（[`Node::Custom`]）命中 `ProjectIr.components` 时展开定义体：
//! 实例 props 作为 `{{名}}` 绑定上下文，实例子元素注入定义体内的 `<Slot/>`；
//! 未定义的组件渲染为 `data-component` 容器（友好降级）。

use std::collections::BTreeMap;
use std::fmt::Write;

use kengine::ir::{
    FieldDef, FieldProps, FieldType, Model, Node, ProjectIr, StatCardProps, TableProps,
};
use serde_json::Value;

use crate::escape::escape_html;

/// 自定义组件展开的最大嵌套深度（防相互引用导致无限递归）。
const MAX_COMPONENT_DEPTH: u32 = 8;

/// 渲染节点列表。
pub(crate) fn render_nodes(
    ir: &ProjectIr,
    nodes: &[Node],
    slot: &str,
    bind: &BTreeMap<String, Value>,
    depth: u32,
) -> String {
    nodes
        .iter()
        .map(|n| render_node(ir, n, slot, bind, depth))
        .collect()
}

/// 渲染单个节点；`slot` 注入到 [`Node::Slot`] 位置，`bind` 提供 `{{名}}` 替换。
pub(crate) fn render_node(
    ir: &ProjectIr,
    node: &Node,
    slot: &str,
    bind: &BTreeMap<String, Value>,
    depth: u32,
) -> String {
    match node {
        Node::Row { children, .. } => {
            let body = render_nodes(ir, children, slot, bind, depth);
            format!("<div class=\"row kv-row\">{body}</div>")
        }
        Node::Col { props, children } => {
            let body = render_nodes(ir, children, slot, bind, depth);
            // Bootstrap 栅格：手机全宽（默认），`md` 断点起按 span/12 分栏
            format!(
                "<div class=\"col-md-{} kv-col\">{body}</div>",
                props.span.clamp(1, 12)
            )
        }
        Node::Section { props, children } => {
            let body = render_nodes(ir, children, slot, bind, depth);
            let title = props
                .title
                .as_deref()
                .map(|t| {
                    format!(
                        "<h3 class=\"card-title\">{}</h3>",
                        escape_html(&bind_str(t, bind))
                    )
                })
                .unwrap_or_default();
            format!(
                "<div class=\"card kv-section\"><div class=\"card-body\">{title}{body}</div></div>"
            )
        }
        Node::Slot => slot.to_string(),
        Node::Form {
            props: form_props,
            children,
        } => {
            let model_name = escape_html(&form_props.model);
            let model = ir.models.get(&form_props.model);
            let fields: String = children
                .iter()
                .map(|c| match c {
                    Node::Field { props } => render_field(model, props, bind),
                    other => render_node(ir, other, slot, bind, depth),
                })
                .collect();
            format!("<form class=\"kv-form\" data-model=\"{model_name}\">{fields}</form>")
        }
        Node::Field { props } => render_field(None, props, bind),
        Node::Table { props, children } => render_table(ir, props, children, bind, depth),
        Node::Column { props } => {
            let label = props
                .label
                .as_deref()
                .map_or_else(|| bind_str(&props.field, bind), |l| bind_str(l, bind));
            format!(
                "<th class=\"kv-th\" data-field=\"{}\">{}</th>",
                escape_html(&props.field),
                escape_html(&label)
            )
        }
        Node::StatCard { props } => render_stat_card(props, bind),
        Node::Text { props } => {
            let body = escape_html(&bind_str(&props.value, bind));
            format!("<span class=\"kv-text\">{body}</span>")
        }
        Node::Custom {
            component,
            props,
            children,
        } => {
            // 已定义组件：展开定义体（props 绑定 `{{名}}`，children 注入 `<Slot/>`）；未定义：兜底容器
            let body = if let Some(def) = ir.components.get(component) {
                if depth >= MAX_COMPONENT_DEPTH {
                    "<div class=\"kv-custom kv-custom-depth\">组件嵌套过深</div>".to_string()
                } else {
                    let inner_slot = render_nodes(ir, children, "", bind, depth + 1);
                    render_node(ir, &def.root, &inner_slot, props, depth + 1)
                }
            } else {
                render_nodes(ir, children, slot, bind, depth)
            };
            format!(
                "<div class=\"kv-custom\" data-component=\"{}\">{body}</div>",
                escape_html(component)
            )
        }
    }
}

/// `{{名}}` 绑定替换：命中实例 props 时替换为属性值，否则原样保留。
fn bind_str(text: &str, bind: &BTreeMap<String, Value>) -> String {
    if !text.contains("{{") || bind.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for (key, value) in bind {
        let pattern = format!("{{{{{key}}}}}");
        if out.contains(&pattern) {
            let replacement = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out = out.replace(&pattern, &replacement);
        }
    }
    out
}

/// 渲染表单字段（Tabler `form-label` / `form-control` / `form-select`）。
fn render_field(
    model: Option<&Model>,
    props: &FieldProps,
    bind: &BTreeMap<String, Value>,
) -> String {
    let field_def = model.and_then(|m| m.fields.iter().find(|f| f.name == props.name));
    let field_type = field_def.map_or(FieldType::Text, |f| f.field_type);
    let label = props
        .label
        .as_deref()
        .or_else(|| field_def.and_then(|f| f.label.as_deref()))
        .unwrap_or(&props.name);
    let label_html = escape_html(&bind_str(label, bind));
    let name = escape_html(&props.name);
    let disabled = if props.readonly { " disabled" } else { "" };
    let required_attr = if props.required { " required" } else { "" };
    let required_mark = if props.required { " *" } else { "" };
    let value_attr = props
        .default
        .as_deref()
        .map(|v| format!(" value=\"{}\"", escape_html(&bind_str(v, bind))))
        .unwrap_or_default();

    let control = match field_type {
        FieldType::Textarea => {
            let rows = props
                .rows
                .map(|r| format!(" rows=\"{r}\""))
                .unwrap_or_default();
            let body = escape_html(&bind_str(props.default.as_deref().unwrap_or(""), bind));
            format!(
                "<textarea class=\"form-control kv-control\" name=\"{name}\"{rows}{disabled}{required_attr}>{body}</textarea>"
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
                "<select class=\"form-select kv-control\" name=\"{name}\"{disabled}{required_attr}>{options_html}</select>"
            )
        }
        FieldType::Boolean => {
            format!(
                "<input class=\"form-check-input kv-control\" type=\"checkbox\" name=\"{name}\"{disabled} />"
            )
        }
        FieldType::Number => {
            format!(
                "<input class=\"form-control kv-control\" type=\"number\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
            )
        }
        FieldType::Date => {
            format!(
                "<input class=\"form-control kv-control\" type=\"date\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
            )
        }
        FieldType::File => {
            format!(
                "<input class=\"form-control kv-control\" type=\"file\" name=\"{name}\"{disabled}{required_attr} />"
            )
        }
        _ => format!(
            "<input class=\"form-control kv-control\" type=\"text\" name=\"{name}\"{value_attr}{disabled}{required_attr} />"
        ),
    };

    let suffix = props
        .suffix
        .as_deref()
        .map(|s| {
            format!(
                "<span class=\"input-group-text kv-suffix\">{}</span>",
                escape_html(&bind_str(s, bind))
            )
        })
        .unwrap_or_default();
    let control_html = if suffix.is_empty() {
        control
    } else {
        format!("<div class=\"input-group\">{control}{suffix}</div>")
    };

    format!(
        "<div class=\"mb-3 kv-field\"><label class=\"form-label kv-label\">{label_html}{required_mark}</label>{control_html}</div>"
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

/// 渲染数据表格：`table-responsive`（移动端横向滚动）+ 表头 + 空态。
fn render_table(
    ir: &ProjectIr,
    props: &TableProps,
    children: &[Node],
    bind: &BTreeMap<String, Value>,
    depth: u32,
) -> String {
    let head: String = children
        .iter()
        .map(|c| render_node(ir, c, "", bind, depth))
        .collect();
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
        "<div class=\"table-responsive kv-table-wrap\" data-model=\"{model_name}\">\
         <table class=\"table table-hover kv-table\"><thead><tr>{head}</tr></thead>\
         <tbody>{empty}</tbody></table></div>"
    )
}

/// 渲染统计卡（Tabler `card`）。
fn render_stat_card(props: &StatCardProps, bind: &BTreeMap<String, Value>) -> String {
    let data = props
        .data
        .as_deref()
        .map(|d| escape_html(&bind_str(d, bind)))
        .unwrap_or_default();
    let unit = props
        .unit
        .as_deref()
        .map(|u| escape_html(&bind_str(u, bind)))
        .unwrap_or_default();
    let unit_html = if unit.is_empty() {
        String::new()
    } else {
        format!("<span class=\"text-secondary kv-stat-unit\">{unit}</span>")
    };
    format!(
        "<div class=\"card kv-stat\"><div class=\"card-body text-center\">\
         <div class=\"display-4 fw-bold kv-stat-data\">{data}</div>{unit_html}</div></div>"
    )
}
