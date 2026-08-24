//! `dev` 开发服务器：扫描 `apps/` → kengine 解析 → kengine-html 渲染 → 浏览器预览。
//!
//! 按 D3/D4 决策：axum + notify + tokio；文件变更后整页刷新（轮询自动刷新，非 SSE/WS）。

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, mpsc};

use anyhow::Context;
use axum::Router;
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use kengine::ir::ProjectIr;
use kengine::{SourceFile, build_project};
use kengine_html::render_page;
use notify::Watcher;

/// 构建状态：解析后的 `IR` + 版本号（用于自动刷新）。
struct BuildState {
    /// 当前 `IR`。
    ir: ProjectIr,
    /// 递增版本号，每次重建 +1。
    version: u64,
}

/// 服务器共享状态。
#[derive(Clone)]
struct AppState {
    /// 项目根目录（包含 `apps/` 与 `public/`）。
    dir: PathBuf,
    /// 当前构建状态。
    build: Arc<RwLock<BuildState>>,
}

/// 启动开发服务器。
pub async fn run(dir: &Path, port: u16) -> anyhow::Result<()> {
    let build_state = Arc::new(RwLock::new(
        build(dir).with_context(|| format!("构建项目失败：{}", dir.display()))?,
    ));
    spawn_watcher(dir.to_path_buf(), build_state.clone());

    let routes: Vec<String> = build_state
        .read()
        .expect("读取状态失败")
        .ir
        .pages
        .iter()
        .map(|p| p.path.clone())
        .collect();

    let state = AppState {
        dir: dir.to_path_buf(),
        build: build_state,
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/__kenvoy/version", get(version))
        .route("/{*path}", get(page_or_static))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("kenvoy dev 服务器已启动：http://{addr}");
    for route in &routes {
        println!("  http://{addr}{route}");
    }
    println!("修改 apps/ 下的 XML 会自动刷新。Ctrl+C 停止。");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 扫描 `apps/` 并构建 `IR`。
fn build(dir: &Path) -> anyhow::Result<BuildState> {
    let mut files = Vec::new();
    collect_source_files(dir, &mut files)?;
    let ir = build_project(&files, "kenvoy")?;
    Ok(BuildState { ir, version: 0 })
}

/// 递归收集项目根下 `apps/` 中的 `.kenvoy.xml` 文件。
fn collect_source_files(dir: &Path, out: &mut Vec<SourceFile>) -> std::io::Result<()> {
    let apps_dir = dir.join("apps");
    if !apps_dir.is_dir() {
        return Ok(());
    }
    walk_dir(&apps_dir, dir, out)
}

fn walk_dir(dir: &Path, base: &Path, out: &mut Vec<SourceFile>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, base, out)?;
        } else if path.extension().is_some_and(|e| e == "xml") {
            let rel = path
                .strip_prefix(base)
                .expect("路径前缀不匹配")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path)?;
            out.push(SourceFile { path: rel, content });
        }
    }
    Ok(())
}

/// 启动文件监听：XML 变更（或目录结构增删）→ 合并事件 → 重建 `IR`。
fn spawn_watcher(dir: PathBuf, state: Arc<RwLock<BuildState>>) {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let relevant = event
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "xml"))
                || matches!(
                    event.kind,
                    notify::EventKind::Create(_) | notify::EventKind::Remove(_)
                );
            if relevant {
                let _ = tx.send(());
            }
        }
    })
    .expect("创建文件监听失败");
    watcher
        .watch(&dir, notify::RecursiveMode::Recursive)
        .expect("开始监听失败");

    std::thread::spawn(move || {
        let _watcher = watcher;
        loop {
            if rx.recv().is_err() {
                break;
            }
            // 合并短时间内的连续事件（编辑器保存常触发多次）
            while rx.try_recv().is_ok() {}
            std::thread::sleep(std::time::Duration::from_millis(200));
            while rx.try_recv().is_ok() {}
            rebuild(&dir, &state);
        }
    });
}

/// 重建 `IR`：成功则替换并递增版本；失败则保留旧版本并打印错误。
fn rebuild(dir: &Path, state: &RwLock<BuildState>) {
    let next_version = state
        .read()
        .expect("读取状态失败")
        .version
        .saturating_add(1);
    match build(dir) {
        Ok(new_state) => {
            let mut s = state.write().expect("写入状态失败");
            s.ir = new_state.ir;
            s.version = next_version;
            println!("已重新构建（版本 {next_version}）");
        }
        Err(err) => {
            eprintln!("构建失败，继续使用上一版本：{err}");
        }
    }
}

/// 首页：页面索引。
async fn index(State(state): State<AppState>) -> Html<String> {
    let ir = &state.build.read().expect("读取状态失败").ir;
    let mut links = String::new();
    for page in &ir.pages {
        let path = escape_html(&page.path);
        let title = escape_html(page.title.as_deref().unwrap_or(&page.path));
        let _ = write!(links, "<li><a href=\"{path}\">{title}</a></li>");
    }
    let html = format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>Kenvoy dev</title></head>\
         <body><h1>Kenvoy dev</h1><p>选择页面：</p><ul>{links}</ul></body></html>"
    );
    Html(html)
}

/// 构建版本号（供自动刷新轮询）。
async fn version(State(state): State<AppState>) -> String {
    state
        .build
        .read()
        .expect("读取状态失败")
        .version
        .to_string()
}

/// 页面路由：命中页面 → 渲染 HTML；否则尝试 `public/` 静态资源。
async fn page_or_static(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    let path_norm = normalize(&path);

    let (html, version) = {
        let s = state.build.read().expect("读取状态失败");
        match s.ir.pages.iter().find(|p| p.path == path_norm) {
            Some(page) => (Some(render_page(&s.ir, page)), s.version),
            None => (None, s.version),
        }
    };

    match html {
        Some(html) => Html(inject_reload(&html, version)).into_response(),
        None => match serve_static(&state.dir, &path_norm) {
            Some(resp) => resp,
            None => (StatusCode::NOT_FOUND, "未找到页面或静态资源").into_response(),
        },
    }
}

/// 归一化请求路径（axum 通配符捕获不带前导 `/`）。
fn normalize(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// 从 `public/` 提供静态资源（防目录穿越）。
fn serve_static(dir: &Path, path: &str) -> Option<Response> {
    let public_dir = dir.join("public");
    let rel = path.trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    let file_path = public_dir.join(rel);
    if !file_path.starts_with(&public_dir) {
        return None;
    }
    let data = std::fs::read(&file_path).ok()?;
    let ctype = content_type(&file_path);
    Some(([(header::CONTENT_TYPE, ctype)], data).into_response())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 注入自动刷新脚本：轮询版本号，变化时整页刷新。
fn inject_reload(html: &str, version: u64) -> String {
    let script = format!(
        "<script>let v={version};setInterval(async()=>{{const r=await fetch('/__kenvoy/version');if((await r.text())!==String(v))location.reload();}},1000);</script>"
    );
    html.replace("</body>", &format!("{script}</body>"))
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

    fn example_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/leave-app")
    }

    #[test]
    fn builds_example_project() {
        let state = build(&example_dir()).expect("构建示例项目失败");
        assert!(state.ir.pages.iter().any(|p| p.path == "/leave/apply"));
        assert!(state.ir.pages.iter().any(|p| p.path == "/leave/list"));
        assert!(state.ir.pages.iter().any(|p| p.path == "/dashboard"));
        assert!(state.ir.layouts.contains_key("/"));
    }

    #[test]
    fn collect_finds_only_kenvoy_xml() {
        let mut files = Vec::new();
        collect_source_files(&example_dir(), &mut files).expect("收集失败");
        assert!(
            files
                .iter()
                .any(|f| f.path == "apps/leave/apply/page.kenvoy.xml")
        );
        assert!(files.iter().all(|f| f.path.starts_with("apps/")));
    }

    #[test]
    fn inject_reload_appends_script() {
        let html = "<html><body>hi</body></html>";
        let out = inject_reload(html, 3);
        assert!(out.contains("__kenvoy/version"));
        assert!(out.contains("location.reload()"));
        assert!(out.ends_with("</html>"));
    }

    #[test]
    fn normalize_path_adds_slash() {
        assert_eq!(normalize("leave/apply"), "/leave/apply");
        assert_eq!(normalize("/leave/apply"), "/leave/apply");
    }

    #[test]
    fn watcher_rebuilds_on_xml_change() {
        // 临时项目：根布局 + 一个页面
        let dir = std::env::temp_dir().join(format!("kenvoy-dev-test-{}", std::process::id()));
        let apps = dir.join("apps");
        std::fs::create_dir_all(&apps).expect("创建临时项目失败");
        std::fs::write(
            apps.join("layout.kenvoy.xml"),
            "<Layout><Root><Slot/></Root></Layout>",
        )
        .expect("写入布局失败");
        let page_dir = apps.join("apply");
        std::fs::create_dir_all(&page_dir).expect("创建页面目录失败");
        let page_file = page_dir.join("page.kenvoy.xml");
        std::fs::write(
            &page_file,
            "<Page><Content><Text value=\"v1\"/></Content></Page>",
        )
        .expect("写入页面失败");

        let build_state = Arc::new(RwLock::new(build(&dir).expect("构建失败")));
        spawn_watcher(dir.clone(), build_state.clone());

        // 等 watcher 就绪后修改 XML
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::fs::write(
            &page_file,
            "<Page><Content><Text value=\"v2\"/></Content></Page>",
        )
        .expect("修改页面失败");

        // 轮询等待版本递增（热重建完成）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut changed = false;
        while std::time::Instant::now() < deadline {
            if build_state.read().expect("读取失败").version > 0 {
                changed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(changed, "watcher 未检测到文件变更");

        let ir = &build_state.read().expect("读取失败").ir;
        let page = ir
            .pages
            .iter()
            .find(|p| p.path == "/apply")
            .expect("页面缺失");
        assert!(render_page(ir, page).contains("v2"), "重建后未反映新内容");

        // 清理（watcher 可能占用目录句柄，忽略删除失败）
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn dev_server_serves_pages_and_static() {
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tower::ServiceExt;

        let dir = example_dir();
        let build_state = Arc::new(RwLock::new(build(&dir).expect("构建失败")));
        let state = AppState {
            dir: dir.clone(),
            build: build_state,
        };
        let app = Router::new()
            .route("/", get(index))
            .route("/__kenvoy/version", get(version))
            .route("/{*path}", get(page_or_static))
            .with_state(state);

        // 页面（含布局链嵌套）
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/leave/apply")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains("Kenvoy 请假管理平台"));
        assert!(html.contains("请假信息"));
        assert!(html.contains("__kenvoy/version"));

        // 版本接口
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/__kenvoy/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 静态资源
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/hello.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 404
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
