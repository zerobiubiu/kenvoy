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

use crate::ir::{
    FieldDef, FieldProps, FieldType, Model, Node, ProjectIr, StatCardProps, TableProps,
};
use serde_json::Value;

use crate::html::escape::escape_html;

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
            let body = escape_html(&translate(ir, props, bind));
            format!("<span class=\"kv-text\">{body}</span>")
        }
        Node::FlowViewer { props } => render_flow_viewer(ir, props),
        Node::Chart { props } => {
            // 柱状图容器：数据由客户端运行时经 `/__kenvoy/stats/{model}` 拉取渲染
            format!(
                "<div class=\"kv-chart\" data-model=\"{}\" data-field=\"{}\" data-stat=\"{}\"></div>",
                escape_html(&props.model),
                escape_html(&props.field),
                escape_html(&props.stat)
            )
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

/// 文本内容：`i18n` 键命中翻译表（项目默认语言）时取翻译，否则原值（含 `{{名}}` 绑定）。
fn translate(
    ir: &ProjectIr,
    props: &crate::ir::TextProps,
    bind: &BTreeMap<String, Value>,
) -> String {
    if let Some(key) = &props.i18n
        && let Some(locale) = ir.project.locale.as_deref()
        && let Some(text) = ir.translations.get(locale).and_then(|m| m.get(key))
    {
        return bind_str(text, bind);
    }
    bind_str(&props.value, bind)
}

/// 渲染流程查看器：节点步骤条（节点名 + 审批人）。
fn render_flow_viewer(ir: &ProjectIr, props: &crate::ir::FlowViewerProps) -> String {
    let Some(flow) = ir.flows.iter().find(|f| f.name == props.flow) else {
        return format!(
            "<div class=\"kv-flow-viewer\" data-flow=\"{}\">流程不存在</div>",
            escape_html(&props.flow)
        );
    };
    let steps: Vec<String> = flow
        .nodes
        .iter()
        .map(|n| {
            let name = escape_html(&n.name);
            let approver = n
                .approver
                .as_deref()
                .map(|a| format!("<span class=\"kv-flow-approver\">{}</span>", escape_html(a)))
                .unwrap_or_default();
            format!(
                "<li class=\"kv-flow-step\"><span class=\"kv-flow-name\">{name}</span>{approver}</li>"
            )
        })
        .collect();
    let steps = steps.join("");
    format!(
        "<div class=\"kv-flow-viewer\" data-flow=\"{}\"><ol class=\"kv-flow-list\">{steps}</ol></div>",
        escape_html(&props.flow)
    )
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
        FieldType::User => {
            // 运行时按 data-source="users" 拉 /__kenvoy/auth/users 填充选项
            format!(
                "<select class=\"form-select kv-control\" name=\"{name}\" data-type=\"user\" data-source=\"users\"{disabled}{required_attr}><option value=\"\">加载中…</option></select>"
            )
        }
        FieldType::Department => {
            format!(
                "<select class=\"form-select kv-control\" name=\"{name}\" data-type=\"department\" data-source=\"departments\"{disabled}{required_attr}><option value=\"\">加载中…</option></select>"
            )
        }
        FieldType::Text => format!(
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
///
/// 表格元数据（模型 / 行过滤 / 排序 / 分页）以 `data-*` 属性输出，供客户端运行时组装查询。
fn render_table(
    ir: &ProjectIr,
    props: &TableProps,
    children: &[Node],
    bind: &BTreeMap<String, Value>,
    depth: u32,
) -> String {
    let head: String = children
        .iter()
        .map(|c| match c {
            Node::Column { props: col } => {
                let label = col
                    .label
                    .as_deref()
                    .map_or_else(|| col.field.clone(), str::to_string);
                // File 类型列输出 `data-type="file"`，运行时渲染下载链接
                let is_file = ir.models.get(&props.model).is_some_and(|m| {
                    m.fields
                        .iter()
                        .any(|f| f.name == col.field && f.field_type == FieldType::File)
                });
                let type_attr = if is_file { " data-type=\"file\"" } else { "" };
                format!(
                    "<th class=\"kv-th\" data-field=\"{}\"{type_attr}>{}</th>",
                    escape_html(&col.field),
                    escape_html(&label)
                )
            }
            other => render_node(ir, other, "", bind, depth),
        })
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
    // 过滤条件 `字段:值,字段2:值2`；排序 `字段:asc,字段2:desc`
    let filter_attr = props
        .filter
        .iter()
        .map(|(k, v)| format!("{k}:{}", v_to_str(v)))
        .collect::<Vec<_>>()
        .join(",");
    let sort_attr = props
        .sort
        .iter()
        .map(|(k, v)| format!("{k}:{}", v_to_str(v)))
        .collect::<Vec<_>>()
        .join(",");
    let filter_html = if filter_attr.is_empty() {
        String::new()
    } else {
        format!(" data-filter=\"{}\"", escape_html(&filter_attr))
    };
    let sort_html = if sort_attr.is_empty() {
        String::new()
    } else {
        format!(" data-sort=\"{}\"", escape_html(&sort_attr))
    };
    let page_html = props
        .page_size
        .map(|p| format!(" data-page-size=\"{p}\""))
        .unwrap_or_default();
    format!(
        "<div class=\"table-responsive kv-table-wrap\" data-model=\"{model_name}\"{filter_html}{sort_html}{page_html}>\
         <table class=\"table table-hover kv-table\"><thead><tr>{head}</tr></thead>\
         <tbody>{empty}</tbody></table></div>"
    )
}

/// `serde_json` 值 → 纯文本（过滤/排序属性用）。
fn v_to_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
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
