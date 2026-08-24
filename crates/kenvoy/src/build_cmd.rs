//! `kv build`：将项目构建为静态 HTML 产物。
//!
//! 输出到 `dist/`（或配置 `[build].output`）：`[build].public_path` 非空时，
//! 全部产物（页面、索引页、`assets/`、`__kenvoy/`、`public/` 拷贝）统一放在
//! `dist{public_path}/` 下，页面内的自产资源引用与索引链接均带此前缀（子路径部署）；
//! 缺省（根路径）时行为同以往。产物为生产 HTML（不含 dev 自动刷新脚本）。
//!
//! 配置 `[database].url` 时，额外导出各模型数据快照到 `__kenvoy/data/{模型}.json`
//! （静态站数据降级：客户端运行时在数据 API 不可用时读取；快照为公开数据，注意隐私）。

use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::{self, KenvoyConfig};
use crate::dev;

/// 执行构建。DB 可用时附带导出静态数据快照。
pub async fn run(dir: &Path, out_override: Option<&Path>, cfg: &KenvoyConfig) -> Result<()> {
    let project_name = config::resolve_name(dir, cfg, None);
    let ir = dev::build_ir(dir, &project_name, cfg.project.locale.as_deref())?;

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

    // 部署子路径：非空时全部产物放入 dist{prefix}/，资源引用带前缀
    let raw_prefix = cfg.build.public_path.as_deref().unwrap_or("");
    let prefix = format!("/{}", raw_prefix.trim_matches('/')).replace("//", "/");
    let prefix = if prefix == "/" { String::new() } else { prefix };
    let root = if prefix.is_empty() {
        out_dir.clone()
    } else {
        out_dir.join(prefix.trim_start_matches('/'))
    };
    std::fs::create_dir_all(&root)?;

    let root_page_owned = ir.pages.iter().any(|p| p.path == "/");

    let mut count = 0;
    for page in &ir.pages {
        let html = kengine_html::render_page_with_prefix(&ir, page, &prefix);
        // 静态产物注入客户端运行时（表格数据由宿主集成时通过 `__kenvoy/ir.json` 等接入）
        let html = html.replace(
            "</body>",
            &format!("<script src=\"{prefix}/__kenvoy/runtime.js\"></script></body>"),
        );
        let rel = page.path.trim_start_matches('/');
        if rel.is_empty() {
            std::fs::write(root.join("index.html"), html)?;
        } else {
            let page_dir = root.join(rel);
            std::fs::create_dir_all(&page_dir)?;
            std::fs::write(page_dir.join("index.html"), html)?;
        }
        count += 1;
    }

    if !root_page_owned {
        let index = build_index(&ir, &prefix);
        std::fs::write(root.join("index.html"), index)?;
    }

    // 样式、图标与运行时资源
    std::fs::create_dir_all(root.join("assets"))?;
    std::fs::write(root.join("assets/tabler.min.css"), kengine_html::TABLER_CSS)?;
    std::fs::write(root.join("assets/icons.css"), kengine_html::ICONS_CSS)?;
    std::fs::write(
        root.join("assets/tabler-icons.woff2"),
        kengine_html::TABLER_ICONS_WOFF2,
    )?;
    std::fs::create_dir_all(root.join("__kenvoy"))?;
    std::fs::write(root.join("__kenvoy/runtime.js"), kengine_html::RUNTIME_JS)?;
    std::fs::write(
        root.join("__kenvoy/ir.json"),
        serde_json::to_string_pretty(&ir)?,
    )?;

    copy_public(dir, &root)?;

    // 静态站数据快照：`[database].url` 配置时导出各模型数据（运行时降级读取）
    if let Some(url) = &cfg.database.url {
        match export_static_data(url, &ir, &root).await {
            Ok(count) => println!("已导出 {count} 个模型静态数据快照（__kenvoy/data/）"),
            Err(err) => eprintln!("静态数据快照导出失败（跳过）：{err}"),
        }
    }

    println!("构建完成：{}（{} 个页面）", out_dir.display(), count);
    Ok(())
}

/// 导出各模型数据快照（`__kenvoy/data/{模型}.json`，公开内容，注意隐私）。
async fn export_static_data(url: &str, ir: &kengine::ir::ProjectIr, root: &Path) -> Result<usize> {
    let pool = kengine_db::connect(url).await.context("连接数据库失败")?;
    kengine_db::migrate(&pool).await.context("数据库迁移失败")?;
    let data_dir = root.join("__kenvoy/data");
    std::fs::create_dir_all(&data_dir)?;
    let mut count = 0;
    for model in ir.models.values() {
        kengine_db::models::ensure_model_table(&pool, model)
            .await
            .context("确保模型表失败")?;
        let rows = kengine_db::models::list_records(&pool, model)
            .await
            .context("读取模型数据失败")?;
        std::fs::write(
            data_dir.join(format!("{}.json", model.name)),
            serde_json::to_string_pretty(&rows)?,
        )?;
        count += 1;
    }
    Ok(count)
}

/// 生成页面索引页（链接带部署前缀）。
fn build_index(ir: &kengine::ir::ProjectIr, prefix: &str) -> String {
    let mut links = String::new();
    for page in &ir.pages {
        let path = escape_html(&page.path);
        let title = escape_html(page.title.as_deref().unwrap_or(&page.path));
        let _ = write!(links, "<li><a href=\"{prefix}{path}\">{title}</a></li>");
    }
    format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>Kenvoy</title></head>\
         <body><h1>Kenvoy</h1><p>选择页面：</p><ul>{links}</ul></body></html>"
    )
}

/// 拷贝 `public/` 到产物根目录。
fn copy_public(project_dir: &Path, root: &Path) -> Result<()> {
    let public_dir = project_dir.join("public");
    if !public_dir.is_dir() {
        return Ok(());
    }
    copy_dir_recursive(&public_dir, root)
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

    #[tokio::test]
    async fn builds_example_to_dist() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/leave-app");
        let out = std::env::temp_dir().join(format!("kenvoy-build-test-{}", std::process::id()));

        run(&dir, Some(&out), &KenvoyConfig::default())
            .await
            .expect("构建失败");

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
