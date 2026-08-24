//! `kv build`：将项目构建为静态 HTML 产物。
//!
//! 输出到 `dist/`（或配置 `[build].output`）：每个页面一个 `<path>/index.html`，
//! 索引页 + `public/` 静态资源拷贝（`[build].public_path` 决定拷贝到的子目录）。
//! 产物为生产 HTML（不含 dev 自动刷新脚本）。

use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use kengine_html::render_page;

use crate::config::{self, KenvoyConfig};
use crate::dev;

/// 执行构建。
pub fn run(dir: &Path, out_override: Option<&Path>, cfg: &KenvoyConfig) -> Result<()> {
    let project_name = config::resolve_name(dir, cfg, None);
    let ir = dev::build_ir(dir, &project_name)?;

    let out_dir = out_override
        .map(Path::to_path_buf)
        .or_else(|| cfg.build.output.as_deref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("dist"));
    let out_dir = if out_dir.is_absolute() {
        out_dir
    } else {
        dir.join(out_dir)
    };

    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;

    let root_page_owned = ir.pages.iter().any(|p| p.path == "/");

    let mut count = 0;
    for page in &ir.pages {
        let html = render_page(&ir, page);
        let rel = page.path.trim_start_matches('/');
        if rel.is_empty() {
            std::fs::write(out_dir.join("index.html"), html)?;
        } else {
            let page_dir = out_dir.join(rel);
            std::fs::create_dir_all(&page_dir)?;
            std::fs::write(page_dir.join("index.html"), html)?;
        }
        count += 1;
    }

    if !root_page_owned {
        let index = build_index(&ir);
        std::fs::write(out_dir.join("index.html"), index)?;
    }

    copy_public(dir, &out_dir, cfg.build.public_path.as_deref())?;

    println!("构建完成：{}（{} 个页面）", out_dir.display(), count);
    Ok(())
}

/// 生成页面索引页。
fn build_index(ir: &kengine::ir::ProjectIr) -> String {
    let mut links = String::new();
    for page in &ir.pages {
        let path = escape_html(&page.path);
        let title = escape_html(page.title.as_deref().unwrap_or(&page.path));
        let _ = write!(links, "<li><a href=\"{path}\">{title}</a></li>");
    }
    format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>Kenvoy</title></head>\
         <body><h1>Kenvoy</h1><p>选择页面：</p><ul>{links}</ul></body></html>"
    )
}

/// 拷贝 `public/` 到输出目录（`public_path` 决定目标子目录，缺省为根）。
fn copy_public(project_dir: &Path, out_dir: &Path, public_path: Option<&str>) -> Result<()> {
    let public_dir = project_dir.join("public");
    if !public_dir.is_dir() {
        return Ok(());
    }
    let sub = public_path.unwrap_or("/").trim_matches('/');
    let dst = if sub.is_empty() {
        out_dir.to_path_buf()
    } else {
        out_dir.join(sub)
    };
    copy_dir_recursive(&public_dir, &dst)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// 简易 HTML 转义（索引页标题/路径）。
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_example_to_dist() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/leave-app");
        let out = std::env::temp_dir().join(format!("kenvoy-build-test-{}", std::process::id()));

        run(&dir, Some(&out), &KenvoyConfig::default()).expect("构建失败");

        let index = std::fs::read_to_string(out.join("index.html")).expect("缺少索引页");
        assert!(index.contains("/leave/apply"));

        let apply = std::fs::read_to_string(out.join("leave/apply/index.html")).expect("缺少页面");
        assert!(apply.contains("请假信息"));
        // 生产产物不应包含 dev 自动刷新脚本
        assert!(!apply.contains("__kenvoy/version"));

        let static_file = std::fs::read_to_string(out.join("hello.txt")).expect("缺少静态资源");
        assert!(static_file.contains("hello from kenvoy public"));

        let _ = std::fs::remove_dir_all(&out);
    }
}
