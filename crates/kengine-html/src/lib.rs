//! kengine-html — `IR` → HTML 渲染器（纯函数，无 I/O）。
//!
//! 将 [`kengine::ir::ProjectIr`] 渲染为可打开的 HTML 文档。
//! 定位为长期 SSR / 边缘渲染核心；客户端运行时（如 Leptos）在此基础上做水合（hydration）。

mod escape;
mod render;

use kengine::ir::{Page, ProjectIr};

use crate::escape::escape_html;

/// Tabler 样式（MIT，v1.3.0），供 dev / build 输出到 `/assets/tabler.min.css`。
pub const TABLER_CSS: &str = include_str!("../assets/tabler.min.css");

/// 客户端运行时 JS（表格数据渲染 + 表单提交），供 dev / build 输出。
pub const RUNTIME_JS: &str = include_str!("../assets/runtime.js");

/// 渲染完整 HTML 文档（含布局链嵌套与页面内容）。
///
/// `Page.layout_chain` 外层在前：根布局最先渲染，`Slot` 逐层注入内层布局，
/// 最内层 `Slot` 注入页面内容。链中缺失的布局会被跳过（防御性处理，
/// 正常情况下由 [`kengine::build_project`] 保证完整）。
///
/// 样式引用 `/assets/tabler.min.css`，由 dev 服务器 / build 产物提供。
#[must_use]
pub fn render_page(ir: &ProjectIr, page: &Page) -> String {
    let title = escape_html(page.title.as_deref().unwrap_or(&page.path));
    let body = render_body(ir, page);
    format!(
        "<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"UTF-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
         <title>{title}</title>\n\
         <link rel=\"stylesheet\" href=\"/assets/tabler.min.css\">\n\
         <style>\n.kv-form .form-label{{font-weight:600}}\n.kv-empty{{text-align:center;color:#6c757d}}\n.kv-custom{{border:1px dashed #ced4da;border-radius:8px;padding:8px;margin:8px}}\n</style>\n\
         </head>\n<body>\n{body}\n</body>\n</html>\n"
    )
}

/// 渲染页面主体：布局链嵌套 + 页面内容。
fn render_body(ir: &ProjectIr, page: &Page) -> String {
    let mut inner = render::render_node(ir, &page.content, "");
    for layout_path in page.layout_chain.iter().rev() {
        let Some(layout) = ir.layouts.get(layout_path) else {
            continue;
        };
        inner = render::render_node(ir, &layout.root, &inner);
    }
    inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use kengine::{SourceFile, build_project};

    fn build(files: &[(&str, &str)]) -> ProjectIr {
        let files: Vec<SourceFile> = files
            .iter()
            .map(|(path, content)| SourceFile {
                path: (*path).to_string(),
                content: (*content).to_string(),
            })
            .collect();
        build_project(&files, "测试").expect("解析失败")
    }

    const ROOT_LAYOUT: &str = "<Layout><Root><Slot/></Root></Layout>";

    #[test]
    fn page_renders_layout_and_content() {
        let ir = build(&[
            (
                "apps/layout.kenvoy.xml",
                "<Layout><Title>根布局</Title><Root><Text value=\"根导航\"/><Slot/></Root></Layout>",
            ),
            (
                "apps/leave/apply/page.kenvoy.xml",
                "<Page><Title>我的页面</Title><Content><Text value=\"页面内容\"/></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("<title>我的页面</title>"));
        assert!(html.contains("根导航"));
        assert!(html.contains("页面内容"));
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn form_fields_render_by_type() {
        let ir = build(&[
            ("apps/layout.kenvoy.xml", ROOT_LAYOUT),
            (
                "apps/leave/models.kenvoy.xml",
                "<Models><Model name=\"请假单\"><Field name=\"申请人\" type=\"user\"/>\
                 <Field name=\"请假类型\" type=\"select\" options=\"年假,病假\"/>\
                 <Field name=\"请假天数\" type=\"number\"/>\
                 <Field name=\"事由\" type=\"textarea\"/>\
                 <Field name=\"通过\" type=\"boolean\"/></Model></Models>",
            ),
            (
                "apps/leave/apply/page.kenvoy.xml",
                "<Page><Content><Form model=\"请假单\">\
                 <Field name=\"申请人\" readonly=\"true\" default=\"{{currentUser.name}}\"/>\
                 <Field name=\"请假类型\"/>\
                 <Field name=\"请假天数\" suffix=\"天\"/>\
                 <Field name=\"事由\" rows=\"4\"/>\
                 <Field name=\"通过\"/></Form></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("disabled"));
        assert!(html.contains("value=\"{{currentUser.name}}\""));
        assert!(html.contains("<select"));
        assert!(html.contains("<option value=\"年假\">年假</option>"));
        assert!(html.contains("type=\"number\""));
        assert!(html.contains(">天</span>"));
        assert!(html.contains("<textarea"));
        assert!(html.contains("rows=\"4\""));
        assert!(html.contains("type=\"checkbox\""));
    }

    #[test]
    fn table_renders_headers_and_empty() {
        let ir = build(&[
            ("apps/layout.kenvoy.xml", ROOT_LAYOUT),
            (
                "apps/leave/models.kenvoy.xml",
                "<Models><Model name=\"请假单\">\
                 <Field name=\"请假类型\" type=\"select\"/>\
                 <Field name=\"请假天数\" type=\"number\"/></Model></Models>",
            ),
            (
                "apps/leave/list/page.kenvoy.xml",
                "<Page><Content><Table model=\"请假单\">\
                 <Column field=\"请假类型\" label=\"类型\"/>\
                 <Column field=\"请假天数\" label=\"天数\" width=\"80\"/>\
                 </Table></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("<th class=\"kv-th\" data-field=\"请假类型\">类型</th>"));
        assert!(html.contains("<th class=\"kv-th\" data-field=\"请假天数\">天数</th>"));
        assert!(html.contains("暂无数据"));
        assert!(html.contains("colspan=\"2\""));
    }

    #[test]
    fn stat_card_renders() {
        let ir = build(&[
            ("apps/layout.kenvoy.xml", ROOT_LAYOUT),
            (
                "apps/leave/apply/page.kenvoy.xml",
                "<Page><Content><StatCard data=\"统计(请假单.数量)\" unit=\"次\"/></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("统计(请假单.数量)"));
        assert!(html.contains(">次</span>"));
    }

    #[test]
    fn custom_component_renders() {
        let ir = build(&[
            ("apps/layout.kenvoy.xml", ROOT_LAYOUT),
            (
                "apps/leave/apply/page.kenvoy.xml",
                "<Page><Content><MyWidget foo=\"bar\"><Text value=\"内部\"/></MyWidget></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("data-component=\"MyWidget\""));
        assert!(html.contains("内部"));
    }

    #[test]
    fn malicious_text_is_escaped() {
        let ir = build(&[
            ("apps/layout.kenvoy.xml", ROOT_LAYOUT),
            (
                "apps/leave/apply/page.kenvoy.xml",
                "<Page><Content><Text value=\"&lt;script&gt;alert(1)&lt;/script&gt;\"/></Content></Page>",
            ),
        ]);
        let html = render_page(&ir, &ir.pages[0]);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
