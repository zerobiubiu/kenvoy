//! `dev` 开发服务器：扫描 `apps/` → kengine 解析 → kengine-html 渲染 → 浏览器预览。
//!
//! 按 D3/D4 决策：axum + notify + tokio；文件变更后整页刷新（轮询自动刷新，非 SSE/WS）。
//! 需要 `db` + `s3` feature（默认开启）。

#![cfg(all(feature = "db", feature = "s3"))]

use std::fmt::Write;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, mpsc};

use crate::db::models::FilterCond;
use crate::db::users::User;
use crate::db::{FlowStore, ModelStore, StorageStore, UserStore};
use crate::s3::ObjectStore;
use anyhow::Context;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Json, Multipart, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use kengine::html::render_page;
use kengine::ir::{DataFilterRule, Flow as IrFlow, Node, Page, ProjectIr, RoleFilter, Validation};
use kengine::validation;
use kengine::workflow::{self, Decision};
use kengine::{SourceFile, build_project};
use notify::Watcher;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::config::{AuthConfig, MailConfig, StorageConfig};

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
    /// 项目描述（`kenvoy.toml`，可选）。
    description: Option<String>,
    /// 当前构建状态。
    build: Arc<RwLock<BuildState>>,
    /// PostgreSQL 连接池（配置 `[database].url` 时启用；否则回退内存模式）。
    db: Option<PgPool>,
    /// 对象存储（配置 `[storage]` 时启用；否则文件上传不可用）。
    store: Option<ObjectStore>,
    /// 存储配置（上传限制等）。
    storage_cfg: StorageConfig,
    /// 内存数据存储（模型名 → 记录列表，无数据库时的降级模式）。
    data: Arc<Mutex<std::collections::HashMap<String, Vec<Value>>>>,
}

/// 开发服务器启动选项。
#[allow(clippy::struct_excessive_bools)]
pub struct DevOptions<'a> {
    /// 项目根目录。
    pub dir: &'a Path,
    /// 监听地址。
    pub host: &'a str,
    /// 监听端口。
    pub port: u16,
    /// 项目显示名称。
    pub project_name: &'a str,
    /// 项目描述。
    pub description: Option<String>,
    /// 默认语言（`I8n/` 目录名）。
    pub locale: Option<String>,
    /// `[storage]` 配置。
    pub storage: &'a StorageConfig,
    /// `[auth]` 配置。
    pub auth: &'a AuthConfig,
    /// `[mail]` 配置。
    pub mail: &'a MailConfig,
    /// PostgreSQL 连接池（未配置为 `None`）。
    pub db: Option<PgPool>,
}

/// 启动开发服务器。
///
/// # Panics
/// 状态锁读失败时（`expect`）会 panic。
///
/// # Errors
/// 绑定端口失败时返回。
pub async fn run(opts: DevOptions<'_>) -> anyhow::Result<()> {
    let dir = opts.dir;
    let host = opts.host;
    let port = opts.port;
    let project_name = opts.project_name;
    let description = opts.description;
    let storage = opts.storage;
    let auth = opts.auth;
    let mail = opts.mail;
    let db = opts.db;
    let build_state = Arc::new(RwLock::new(
        build(dir, project_name, opts.locale.as_deref())
            .with_context(|| format!("构建项目失败：{}", dir.display()))?,
    ));
    spawn_watcher(
        dir.to_path_buf(),
        project_name.to_string(),
        opts.locale.clone(),
        build_state.clone(),
    );

    let routes: Vec<String> = build_state
        .read()
        .expect("读取状态失败")
        .ir
        .pages
        .iter()
        .map(|p| p.path.clone())
        .collect();

    let db_for_auth = db.clone();
    let store = connect_store(storage).await;
    let state = AppState {
        dir: dir.to_path_buf(),
        description,
        build: build_state,
        db,
        store,
        storage_cfg: storage.clone(),
        data: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/__kenvoy/version", get(version))
        .route("/__kenvoy/runtime.js", get(runtime_js))
        .route("/assets/tabler.min.css", get(tabler_css))
        .route("/assets/icons.css", get(icons_css))
        .route("/assets/tabler-icons.woff2", get(tabler_icons_woff2))
        .route("/__kenvoy/data/{model}", get(list_data).post(create_data))
        .route(
            "/__kenvoy/data/{model}/{id}",
            put(update_data).delete(delete_data),
        )
        .route("/__kenvoy/stats/{model}", get(stats_data))
        .route(
            "/__kenvoy/flow/{flow}/records/{record_id}/start",
            post(start_flow),
        )
        .route(
            "/__kenvoy/flow/{flow}/records/{record_id}/step",
            post(step_flow),
        )
        .route(
            "/__kenvoy/flow/{flow}/records/{record_id}/withdraw",
            post(withdraw_flow),
        )
        .route(
            "/__kenvoy/flow/{flow}/records/{record_id}/add-sign",
            post(add_sign_flow),
        )
        .route(
            "/__kenvoy/flow/{flow}/records/{record_id}",
            get(get_flow_instance),
        )
        .route(
            "/__kenvoy/file",
            post(upload_file).layer(DefaultBodyLimit::max(upload_body_limit(storage))),
        )
        .route("/__kenvoy/file/{id}", get(download_file))
        .nest(
            "/__kenvoy/auth",
            crate::auth::router().with_state(crate::auth::AuthState {
                db: db_for_auth,
                auth: auth.clone(),
                mail: mail.clone(),
            }),
        )
        .route("/{*path}", get(page_or_static))
        .with_state(state);

    let addr = (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("无法解析监听地址 {host}:{port}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("kenvoy dev 服务器已启动：http://{addr}");
    for route in &routes {
        println!("  http://{addr}{route}");
    }
    println!("修改 apps/ 下的 XML 会自动刷新。Ctrl+C 停止。");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 扫描 `apps/` 并解析为 `IR`（供 `dev` / `build` 复用）。
pub(crate) fn build_ir(
    dir: &Path,
    project_name: &str,
    locale: Option<&str>,
) -> anyhow::Result<ProjectIr> {
    let mut files = Vec::new();
    collect_source_files(dir, &mut files)?;
    let mut ir = build_project(&files, project_name)?;
    ir.project.locale = locale.map(str::to_string);
    Ok(ir)
}

fn build(dir: &Path, project_name: &str, locale: Option<&str>) -> anyhow::Result<BuildState> {
    let ir = build_ir(dir, project_name, locale)?;
    Ok(BuildState { ir, version: 0 })
}

/// 递归收集项目根下 `apps/` 中的 `.kenvoy.xml` 文件。
fn collect_source_files(dir: &Path, out: &mut Vec<SourceFile>) -> std::io::Result<()> {
    let apps_dir = dir.join("apps");
    if apps_dir.is_dir() {
        walk_dir(&apps_dir, dir, out)?;
    }
    // 全局 `module/` 自定义组件目录
    let module_dir = dir.join("module");
    if module_dir.is_dir() {
        walk_dir(&module_dir, dir, out)?;
    }
    // 国际化 `I8n/` 目录（区分大小写兼容：i18n / I8n）
    for name in ["I8n", "i18n"] {
        let i18n_dir = dir.join(name);
        if i18n_dir.is_dir() {
            walk_dir(&i18n_dir, dir, out)?;
        }
    }
    // 外部库 `packages/` 目录：每个子目录可包含 XML 文件（与 module/ 类似）
    let packages_dir = dir.join("packages");
    if packages_dir.is_dir() {
        walk_dir(&packages_dir, dir, out)?;
    }
    Ok(())
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
fn spawn_watcher(
    dir: PathBuf,
    project_name: String,
    locale: Option<String>,
    state: Arc<RwLock<BuildState>>,
) {
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
            rebuild(&dir, &project_name, locale.as_deref(), &state);
        }
    });
}

/// 重建 `IR`：成功则替换并递增版本；失败则保留旧版本并打印错误。
fn rebuild(dir: &Path, project_name: &str, locale: Option<&str>, state: &RwLock<BuildState>) {
    let next_version = state
        .read()
        .expect("读取状态失败")
        .version
        .saturating_add(1);
    match build(dir, project_name, locale) {
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
    let desc = state
        .description
        .as_deref()
        .map(|d| format!("<p class=\"kv-desc\">{}</p>", escape_html(d)))
        .unwrap_or_default();
    let mut links = String::new();
    for page in &ir.pages {
        let path = escape_html(&page.path);
        let title = escape_html(page.title.as_deref().unwrap_or(&page.path));
        let _ = write!(links, "<li><a href=\"{path}\">{title}</a></li>");
    }
    let html = format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><title>Kenvoy dev</title></head>\
         <body><h1>Kenvoy dev</h1>{desc}<p>选择页面：</p><ul>{links}</ul></body></html>"
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
        Some(html) => Html(inject_dev_scripts(&html, version)).into_response(),
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

/// 注入 dev 脚本：客户端运行时 + 轮询自动刷新。
fn inject_dev_scripts(html: &str, version: u64) -> String {
    let scripts = format!(
        "<script src=\"/__kenvoy/runtime.js\"></script>\
         <script>let v={version};setInterval(async()=>{{const r=await fetch('/__kenvoy/version');if((await r.text())!==String(v))location.reload();}},1000);</script>"
    );
    html.replace("</body>", &format!("{scripts}</body>"))
}

/// 返回客户端运行时 JS。
async fn runtime_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        kengine::html::RUNTIME_JS,
    )
        .into_response()
}

/// 返回 Tabler 样式。
async fn tabler_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        kengine::html::TABLER_CSS,
    )
        .into_response()
}

/// 返回 Tabler 图标子集样式。
async fn icons_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        kengine::html::ICONS_CSS,
    )
        .into_response()
}

/// 返回 Tabler 图标字体。
async fn tabler_icons_woff2() -> Response {
    (
        [(header::CONTENT_TYPE, "font/woff2")],
        kengine::html::TABLER_ICONS_WOFF2,
    )
        .into_response()
}

/// `[storage]` 配置齐全时连接对象存储并确保 Bucket 存在；失败仅告警（不阻断 dev 服务）。
async fn connect_store(storage: &StorageConfig) -> Option<ObjectStore> {
    let cfg = s3_config(storage)?;
    let store = match ObjectStore::connect(&cfg) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("对象存储配置无效（文件上传未启用）：{err}");
            return None;
        }
    };
    match store.ensure_bucket().await {
        Ok(()) => {
            println!("已连接对象存储（bucket: {}）", store.bucket_name());
            Some(store)
        }
        Err(err) => {
            eprintln!("对象存储启动失败（文件上传未启用）：{err}");
            None
        }
    }
}

/// `[storage]` 配置齐全时转换为 `crate::s3::S3Config`，否则返回 `None`。
fn s3_config(cfg: &StorageConfig) -> Option<crate::s3::S3Config> {
    let s = cfg;
    Some(crate::s3::S3Config {
        endpoint: s.endpoint.clone()?,
        region: s.region.clone()?,
        bucket: s.bucket.clone()?,
        access_key: s.access_key.clone()?,
        secret_key: s.secret_key.clone()?,
    })
}

/// multipart body 上限（避免 axum 默认 2MB 隐藏瓶颈）：
/// 配置 `[storage].max_size_mb` 时取「上限 + 2MB 缓冲」（保证业务层 413 优先）；
/// 未配置时取 512MB（限制决策完全归业务层）。
fn upload_body_limit(storage: &StorageConfig) -> usize {
    storage.max_size_mb.map_or(512 * 1024 * 1024, |mb| {
        usize::try_from(mb.saturating_mul(1024 * 1024))
            .unwrap_or(usize::MAX)
            .saturating_add(2 * 1024 * 1024)
    })
}

/// 上传文件：`POST /__kenvoy/file`（multipart，字段名 `file`）。
///
/// 需登录；文件本体写入 S3，元数据写入 `storage_object`，返回 `{ id, objectKey, fileName }`。
/// 元数据写入失败时删除已上传对象，保持 S3 与元数据一致。
async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(pool) = &state.db else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "文件存储需要配置 PostgreSQL（kenvoy.toml [database].url）",
        )
            .into_response();
    };
    let Some(store) = &state.store else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "文件存储未配置（kenvoy.toml [storage]）",
        )
            .into_response();
    };
    if let Err(resp) = auth_user(&state, &headers).await {
        return resp;
    }
    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return (StatusCode::BAD_REQUEST, "缺少文件字段").into_response(),
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };
    let file_name = field.file_name().unwrap_or("未命名").to_string();
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    let data = match field.bytes().await {
        Ok(d) => d,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("读取文件内容失败：{err}")).into_response();
        }
    };
    if data.is_empty() {
        return (StatusCode::BAD_REQUEST, "文件内容为空").into_response();
    }
    // 上传限制（`[storage]` 配置）：大小上限 / 类型白名单
    if let Some(max_mb) = state.storage_cfg.max_size_mb {
        let max_bytes = max_mb.saturating_mul(1024 * 1024);
        if data.len() as u64 > max_bytes {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("文件大小超过上限 {max_mb} MB"),
            )
                .into_response();
        }
    }
    if !state.storage_cfg.allowed_types.is_empty()
        && !state
            .storage_cfg
            .allowed_types
            .iter()
            .any(|t| type_allowed(&content_type, t))
    {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            format!("不支持的文件类型：{content_type}"),
        )
            .into_response();
    }
    let key = ObjectStore::object_key(&file_name);
    let etag = match store.put(&key, &data, &content_type).await {
        Ok(e) => e,
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    let id = match StorageStore::new(pool)
        .create(
            store.bucket_name(),
            &key,
            &file_name,
            Some(&content_type),
            i64::try_from(data.len()).unwrap_or(i64::MAX),
            etag.as_deref(),
        )
        .await
    {
        Ok(id) => id,
        Err(err) => {
            let _ = store.delete(&key).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    Json(json!({ "id": id, "objectKey": key, "fileName": file_name })).into_response()
}

/// `Content-Type` 是否匹配白名单项（支持 `image/*` 通配）。
fn type_allowed(content_type: &str, rule: &str) -> bool {
    if let Some(prefix) = rule.strip_suffix("/*") {
        content_type.starts_with(&format!("{prefix}/"))
    } else {
        content_type == rule
    }
}

/// 下载文件：`GET /__kenvoy/file/{id}`。
///
/// 需登录；按元数据中的 Content-Type 返回文件本体。
async fn download_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let Some(pool) = &state.db else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "文件存储需要配置 PostgreSQL（kenvoy.toml [database].url）",
        )
            .into_response();
    };
    let Some(store) = &state.store else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "文件存储未配置（kenvoy.toml [storage]）",
        )
            .into_response();
    };
    if let Err(resp) = auth_user(&state, &headers).await {
        return resp;
    }
    let obj = match StorageStore::new(pool).get(&id).await {
        Ok(Some(o)) => o,
        Ok(None) => return (StatusCode::NOT_FOUND, "文件不存在").into_response(),
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    let bytes = match store.get(&obj.object_key).await {
        Ok(b) => b,
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    let ctype = obj
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    ([(header::CONTENT_TYPE, ctype)], bytes).into_response()
}

/// 列表数据：`GET /__kenvoy/data/{model}`。
///
/// 启用数据库时要求登录（Bearer token），并按 `DataFilter` 做行级过滤；
/// 支持查询参数：`filter`（`字段:值,字段2:值2`，与权限条件 AND）、`sort`（`字段:asc`）、`pageSize`。
async fn list_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(model): AxumPath<String>,
    axum::extract::Query(params): axum::extract::Query<DataListQuery>,
) -> Response {
    if let Some(pool) = &state.db {
        let (user, roles) = match auth_user(&state, &headers).await {
            Ok(v) => v,
            Err(r) => return r,
        };
        let model_def = {
            let build = state.build.read().expect("读取状态失败");
            let Some(m) = build.ir.models.get(&model) else {
                return (StatusCode::NOT_FOUND, "模型不存在").into_response();
            };
            m.clone()
        };
        let store = ModelStore::new(pool);
        if let Err(err) = store.ensure(&model_def).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        let mut groups = {
            let build = state.build.read().expect("读取状态失败");
            match resolve_data_filter(&build.ir, &model, &roles, &user) {
                FilterDecision::AllowAll => Vec::new(),
                FilterDecision::Forbidden => {
                    return (StatusCode::FORBIDDEN, "无数据访问权限").into_response();
                }
                FilterDecision::Conditions(g) => g,
            }
        };
        // 用户查询条件：仅接受模型字段或 `id`（并入每组权限条件 AND）。
        // 值支持 `{{currentUser.*}}` 绑定，运行时替换为当前用户上下文。
        let ctx = user_context(&user);
        let user_conds: Vec<FilterCond> = params
            .filter
            .as_deref()
            .map(parse_filter_pairs)
            .into_iter()
            .flatten()
            .filter(|(f, _)| f == "id" || model_def.fields.iter().any(|m| m.name == *f))
            .map(|(f, v)| FilterCond::eq(&f, &resolve_binding(&v, &ctx)))
            .collect();
        if !user_conds.is_empty() {
            if groups.is_empty() {
                groups = vec![user_conds];
            } else {
                for g in &mut groups {
                    g.extend(user_conds.iter().cloned());
                }
            }
        }
        let order_by: Vec<(String, String)> = params
            .sort
            .as_deref()
            .map(parse_sort_pairs)
            .unwrap_or_default()
            .into_iter()
            .filter(|(f, _)| model_def.fields.iter().any(|m| m.name == *f))
            .collect();
        match store
            .list_query(&model_def, &groups, &order_by, params.page_size)
            .await
        {
            Ok(rows) => {
                let rows = {
                    let build = state.build.read().expect("读取状态失败");
                    apply_field_read_perms(rows, &model_def, &build.ir, &roles)
                };
                Json(rows).into_response()
            }
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        }
    } else {
        let records = state
            .data
            .lock()
            .expect("读取数据失败")
            .get(&model)
            .cloned()
            .unwrap_or_default();
        Json(records).into_response()
    }
}

/// 数据列表查询参数。
#[derive(serde::Deserialize)]
struct DataListQuery {
    /// `字段:值,字段2:值2`。
    filter: Option<String>,
    /// `字段:asc,字段2:desc`。
    sort: Option<String>,
    /// 每页条数（缺省 500，上限 5000）。
    #[serde(rename = "pageSize")]
    page_size: Option<i64>,
}

/// 解析 `字段:值,字段2:值2`。
fn parse_filter_pairs(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let (f, v) = p.split_once(':')?;
            (!f.is_empty()).then(|| (f.trim().to_string(), v.to_string()))
        })
        .collect()
}

/// 解析 `字段:asc,字段2:desc`（未知方向回退 `asc`）。
fn parse_sort_pairs(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let (f, v) = p.split_once(':')?;
            let dir = if v.eq_ignore_ascii_case("desc") {
                "desc"
            } else {
                "asc"
            };
            (!f.is_empty()).then(|| (f.trim().to_string(), dir.to_string()))
        })
        .collect()
}

/// 分组统计：`GET /__kenvoy/stats/{model}?group=字段&stat=count|sum|avg`（需登录）。
async fn stats_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(model): AxumPath<String>,
    axum::extract::Query(params): axum::extract::Query<StatsQuery>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    if let Err(r) = auth_user(&state, &headers).await {
        return r;
    }
    let Some(group) = params.group.as_deref() else {
        return (StatusCode::BAD_REQUEST, "缺少 group 参数").into_response();
    };
    let stat = params.stat.as_deref().unwrap_or("count");
    if !matches!(stat, "count" | "sum" | "avg") {
        return (StatusCode::BAD_REQUEST, "stat 仅支持 count/sum/avg").into_response();
    }
    let model_def = {
        let build = state.build.read().expect("读取状态失败");
        let Some(m) = build.ir.models.get(&model) else {
            return (StatusCode::NOT_FOUND, "模型不存在").into_response();
        };
        if !m.fields.iter().any(|f| f.name == group) {
            return (StatusCode::BAD_REQUEST, "group 字段不属于该模型").into_response();
        }
        m.clone()
    };
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    match crate::db::models::stats_group(pool, &model_def, group, stat).await {
        Ok(rows) => Json(rows).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct StatsQuery {
    /// 分组字段。
    group: Option<String>,
    /// 统计方式（缺省 `count`）。
    stat: Option<String>,
}

/// 创建数据：`POST /__kenvoy/data/{model}`。
///
/// 启用数据库时要求登录，先执行表单校验（`<Validation>`），再校验字段写权限，
/// 失败均返回 400。
async fn create_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(model): AxumPath<String>,
    Json(payload): Json<Value>,
) -> Response {
    if let Some(pool) = &state.db {
        let (_, roles) = match auth_user(&state, &headers).await {
            Ok(v) => v,
            Err(r) => return r,
        };
        let (model_def, validations) = {
            let build = state.build.read().expect("读取状态失败");
            let Some(m) = build.ir.models.get(&model) else {
                return (StatusCode::NOT_FOUND, "模型不存在").into_response();
            };
            (m.clone(), collect_validations(&build.ir.pages, &model))
        };
        let data_map: std::collections::HashMap<String, Value> = payload
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let expr_data: std::collections::HashMap<String, kengine::expr::Value> = data_map
            .iter()
            .map(|(k, v)| (k.clone(), json_to_expr_value(v)))
            .collect();
        let errors = validation::validate(&validations, &expr_data);
        if !errors.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))).into_response();
        }
        // 字段写权限：任一相关 publish 显式禁止该字段写入 → 拒绝
        let denied: Vec<&String> = data_map
            .keys()
            .filter(|k| {
                let build = state.build.read().expect("读取状态失败");
                !field_write_allowed(&build.ir, k, &roles)
            })
            .collect();
        if !denied.is_empty() {
            let names: Vec<&str> = denied.iter().map(|s| s.as_str()).collect();
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "errors": [format!("无字段写入权限：{}", names.join(", "))] })),
            )
                .into_response();
        }
        let store = ModelStore::new(pool);
        if let Err(err) = store.ensure(&model_def).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        match store.insert(&model_def, &data_map).await {
            Ok(_) => Json(payload).into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        }
    } else {
        let mut data = state.data.lock().expect("写入数据失败");
        data.entry(model).or_default().push(payload.clone());
        Json(payload).into_response()
    }
}

/// 更新数据：`PUT /__kenvoy/data/{model}/{id}`（与创建相同的鉴权与校验）。
async fn update_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((model, id)): AxumPath<(String, i64)>,
    Json(payload): Json<Value>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    let (_, roles) = match auth_user(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (model_def, validations) = {
        let build = state.build.read().expect("读取状态失败");
        let Some(m) = build.ir.models.get(&model) else {
            return (StatusCode::NOT_FOUND, "模型不存在").into_response();
        };
        (m.clone(), collect_validations(&build.ir.pages, &model))
    };
    let data_map: std::collections::HashMap<String, Value> = payload
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    let expr_data: std::collections::HashMap<String, kengine::expr::Value> = data_map
        .iter()
        .map(|(k, v)| (k.clone(), json_to_expr_value(v)))
        .collect();
    let errors = validation::validate(&validations, &expr_data);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))).into_response();
    }
    let denied: Vec<&String> = data_map
        .keys()
        .filter(|k| {
            let build = state.build.read().expect("读取状态失败");
            !field_write_allowed(&build.ir, k, &roles)
        })
        .collect();
    if !denied.is_empty() {
        let names: Vec<&str> = denied.iter().map(|s| s.as_str()).collect();
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "errors": [format!("无字段写入权限：{}", names.join(", "))] })),
        )
            .into_response();
    }
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    match store.update(&model_def, id, &data_map).await {
        Ok(true) => Json(payload).into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "记录不存在").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 删除数据：`DELETE /__kenvoy/data/{model}/{id}`。
async fn delete_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((model, id)): AxumPath<(String, i64)>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    if let Err(r) = auth_user(&state, &headers).await {
        return r;
    }
    let model_def = {
        let build = state.build.read().expect("读取状态失败");
        let Some(m) = build.ir.models.get(&model) else {
            return (StatusCode::NOT_FOUND, "模型不存在").into_response();
        };
        m.clone()
    };
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    match store.delete(&model_def, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "记录不存在").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 流程起点（`flow.trigger` 首段为模型名，如 `请假单.提交申请`）。
fn flow_model(flow: &IrFlow) -> Option<&str> {
    flow.trigger.as_deref().and_then(|t| t.split('.').next())
}

/// 审批人校验：节点 `approver`（可带 `角色.` 前缀）匹配当前用户角色；未声明则放行。
fn approver_allows(node: &kengine::ir::FlowNode, roles: &[String]) -> bool {
    let Some(approver) = node.approver.as_deref() else {
        return true;
    };
    let role = approver.strip_prefix("角色.").unwrap_or(approver);
    roles.iter().any(|r| r == role)
}

/// 应用状态机副作用：模型字段更新落库。
async fn apply_flow_effects(
    store: &ModelStore<'_>,
    model: &kengine::ir::Model,
    record_id: i64,
    effects: &workflow::Effects,
) -> Result<(), String> {
    for (field, value) in &effects.updates {
        let mut data = std::collections::HashMap::new();
        data.insert(field.clone(), Value::String(value.clone()));
        store
            .update(model, record_id, &data)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 连续执行知会节点（`notify` 无审批人，自动推进直到普通节点或结束）。
#[allow(clippy::too_many_arguments)] // dev 内部装配函数，直传比再包一层结构体更直白
async fn auto_run_notify(
    state: &AppState,
    pool: &PgPool,
    flow: &IrFlow,
    model_def: &kengine::ir::Model,
    record_id: i64,
    instance_id: i64,
    mut current: Option<String>,
    user: &User,
) -> (Option<String>, String) {
    let mut steps = 0;
    while let Some(node_name) = current.clone()
        && steps < 20
    {
        let Some(node) = flow.nodes.iter().find(|n| n.name == node_name) else {
            break;
        };
        if node.kind != kengine::ir::FlowNodeKind::Notify {
            break;
        }
        steps += 1;
        let (data, ctx) = load_record_context(state, model_def, record_id, user).await;
        let mut data = data;
        let Ok(outcome) = workflow::step(flow, &node_name, Decision::Approve, &mut data, &ctx)
        else {
            break;
        };
        if let Err(err) = apply_flow_effects(
            &ModelStore::new(pool),
            model_def,
            record_id,
            &outcome.effects,
        )
        .await
        {
            eprintln!("流程副作用落库失败：{err}");
            break;
        }
        current = outcome.next;
    }
    if steps == 20 {
        eprintln!("流程自动推进超过 20 步，停止");
    }
    let status = if current.is_none() { "done" } else { "running" };
    let _ = FlowStore::new(pool)
        .advance(instance_id, current.as_deref().unwrap_or(""), status)
        .await;
    (current, status.to_string())
}

/// 加载记录数据为表达式上下文（`{{字段}}` + `currentUser.*`）。
async fn load_record_context(
    state: &AppState,
    model_def: &kengine::ir::Model,
    record_id: i64,
    user: &User,
) -> (
    std::collections::HashMap<String, kengine::expr::Value>,
    kengine::expr::Context,
) {
    let mut data = std::collections::HashMap::new();
    let Some(pool) = &state.db else {
        return (data, user_context(user));
    };
    let store = ModelStore::new(pool);
    if let Ok(Some(row)) = find_record(store, model_def, record_id).await
        && let Value::Object(map) = row
    {
        for (k, v) in map {
            data.insert(k, json_to_expr_value(&v));
        }
    }
    (data, user_context(user))
}

/// 查记录（按 id 过滤）。
async fn find_record(
    store: ModelStore<'_>,
    model_def: &kengine::ir::Model,
    record_id: i64,
) -> Result<Option<Value>, String> {
    // 复用行级过滤：id 字段 + Eq 条件
    let groups = vec![vec![FilterCond::eq("id", &record_id.to_string())]];
    store
        .list_filtered(model_def, &groups)
        .await
        .map(|mut rows| rows.pop())
        .map_err(|e| e.to_string())
}

/// 启动流程：`POST /__kenvoy/flow/{flow}/records/{record_id}/start`。
async fn start_flow(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((flow_name, record_id)): AxumPath<(String, i64)>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    let (user, _roles) = match auth_user(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (flow, model_def, model_name) = {
        let build = state.build.read().expect("读取状态失败");
        let Some(flow) = build.ir.flows.iter().find(|f| f.name == flow_name) else {
            return (StatusCode::NOT_FOUND, "流程不存在").into_response();
        };
        let Some(model_name) = flow_model(flow) else {
            return (
                StatusCode::BAD_REQUEST,
                "流程未声明触发模型（flow.trigger）",
            )
                .into_response();
        };
        let Some(m) = build.ir.models.get(model_name) else {
            return (StatusCode::NOT_FOUND, "触发模型不存在").into_response();
        };
        (flow.clone(), m.clone(), model_name.to_string())
    };
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    let Some(_) = find_record(store, &model_def, record_id)
        .await
        .ok()
        .flatten()
    else {
        return (StatusCode::NOT_FOUND, "记录不存在").into_response();
    };
    let Some(start) = workflow::start_node(&flow) else {
        return (StatusCode::BAD_REQUEST, "流程无节点").into_response();
    };
    let instance_id = match FlowStore::new(pool)
        .create(&flow.name, &model_name, record_id, &start, Some(user.id))
        .await
    {
        Ok(id) => id,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    // 首节点为知会节点时自动推进
    let (current, status) = auto_run_notify(
        &state,
        pool,
        &flow,
        &model_def,
        record_id,
        instance_id,
        Some(start),
        &user,
    )
    .await;
    Json(json!({
        "instanceId": instance_id,
        "currentNode": current,
        "status": status,
    }))
    .into_response()
}

/// 流程推进：`POST /__kenvoy/flow/{flow}/records/{record_id}/step`。
///
/// body：`{"action": "approve" | "reject"}`。
async fn step_flow(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((flow_name, record_id)): AxumPath<(String, i64)>,
    Json(req): Json<FlowStepReq>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    let (user, roles) = match auth_user(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (flow, model_def, model_name) = {
        let build = state.build.read().expect("读取状态失败");
        let Some(flow) = build.ir.flows.iter().find(|f| f.name == flow_name) else {
            return (StatusCode::NOT_FOUND, "流程不存在").into_response();
        };
        let Some(model_name) = flow_model(flow) else {
            return (StatusCode::BAD_REQUEST, "流程未声明触发模型").into_response();
        };
        let Some(m) = build.ir.models.get(model_name) else {
            return (StatusCode::NOT_FOUND, "触发模型不存在").into_response();
        };
        (flow.clone(), m.clone(), model_name.to_string())
    };
    let flow_store = FlowStore::new(pool);
    let instance = match flow_store.latest_by_record(&model_name, record_id).await {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::NOT_FOUND, "流程未启动").into_response(),
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    if instance.status != "running" {
        return (
            StatusCode::CONFLICT,
            format!("流程已结束（{}）", instance.status),
        )
            .into_response();
    }
    let Some(node) = flow.nodes.iter().find(|n| n.name == instance.current_node) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "当前节点不存在").into_response();
    };
    // 审批人校验（`approver` 为角色名，可带 `角色.` 前缀）
    if !approver_allows(node, &roles) {
        return (StatusCode::FORBIDDEN, "当前用户不是本节点审批人").into_response();
    }
    let decision = match req.action.as_str() {
        "approve" => Decision::Approve,
        "reject" => Decision::Reject,
        other => {
            return (StatusCode::BAD_REQUEST, format!("无效决策：{other}")).into_response();
        }
    };
    let mut data = std::collections::HashMap::new();
    let mut ctx = user_context(&user);
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    if let Ok(Some(row)) = find_record(ModelStore::new(pool), &model_def, record_id).await
        && let Value::Object(map) = row
    {
        for (k, v) in map {
            let expr_v = json_to_expr_value(&v);
            ctx.insert(k.as_str(), expr_v.clone());
            data.insert(k, expr_v);
        }
    }
    let outcome = match workflow::step(&flow, &instance.current_node, decision, &mut data, &ctx) {
        Ok(o) => o,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, err.message.clone()).into_response();
        }
    };
    if let Err(err) = apply_flow_effects(&store, &model_def, record_id, &outcome.effects).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response();
    }
    let (current, status) = auto_run_notify(
        &state,
        pool,
        &flow,
        &model_def,
        record_id,
        instance.id,
        outcome.next,
        &user,
    )
    .await;
    Json(json!({
        "instanceId": instance.id,
        "currentNode": current,
        "status": status,
        "notifications": outcome.effects.notifications,
        "messages": outcome.effects.messages,
    }))
    .into_response()
}

#[derive(serde::Deserialize)]
struct FlowStepReq {
    action: String,
}

/// 流程撤回：`POST /__kenvoy/flow/{flow}/records/{record_id}/withdraw`。
///
/// 仅发起人可调用；`running` → `withdrawn`。
async fn withdraw_flow(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((flow_name, record_id)): AxumPath<(String, i64)>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    let (user, _roles) = match auth_user(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let model_name = flow_model_in_ir(&state, &flow_name).unwrap_or_default();
    if model_name.is_empty() {
        return (StatusCode::NOT_FOUND, "流程不存在").into_response();
    }
    let flow_store = FlowStore::new(pool);
    let instance = match flow_store.latest_by_record(&model_name, record_id).await {
        Ok(Some(i)) => i,
        Ok(None) => return (StatusCode::NOT_FOUND, "流程未启动").into_response(),
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
    };
    if instance.created_by != Some(user.id) {
        return (StatusCode::FORBIDDEN, "仅发起人可撤回").into_response();
    }
    match flow_store.withdraw(instance.id).await {
        Ok(()) => Json(json!({ "instanceId": instance.id, "status": "withdrawn" })).into_response(),
        Err(err) => (StatusCode::CONFLICT, err.to_string()).into_response(),
    }
}

/// 流程加签：`POST /__kenvoy/flow/{flow}/records/{record_id}/add-sign`（body `{"node":"节点名"}`）。
///
/// 另起一个实例从指定节点开始，常用于“请主管加签法律审查”。
async fn add_sign_flow(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((flow_name, record_id)): AxumPath<(String, i64)>,
    Json(req): Json<AddSignReq>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    let (user, roles) = match auth_user(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    // 仅当前流程的 running 实例的发起人或审批人可加签
    let model_name = flow_model_in_ir(&state, &flow_name).unwrap_or_default();
    if model_name.is_empty() {
        return (StatusCode::NOT_FOUND, "流程不存在").into_response();
    }
    let (flow, model_def) = {
        let build = state.build.read().expect("读取状态失败");
        let Some(flow) = build.ir.flows.iter().find(|f| f.name == flow_name).cloned() else {
            return (StatusCode::NOT_FOUND, "流程不存在").into_response();
        };
        let Some(m) = build.ir.models.get(&model_name).cloned() else {
            return (StatusCode::NOT_FOUND, "触发模型不存在").into_response();
        };
        (flow, m)
    };
    let flow_store = FlowStore::new(pool);
    if let Ok(Some(inst)) = flow_store.latest_by_record(&model_name, record_id).await
        && inst.status == "running"
    {
        let allowed = inst.created_by == Some(user.id)
            || flow
                .nodes
                .iter()
                .any(|n| n.name == inst.current_node && approver_allows(n, &roles));
        if !allowed {
            return (StatusCode::FORBIDDEN, "无加签权限").into_response();
        }
    }
    let Some(node) = flow.nodes.iter().find(|n| n.name == req.node) else {
        return (StatusCode::BAD_REQUEST, "节点不存在").into_response();
    };
    if !approver_allows(node, &roles) {
        return (StatusCode::FORBIDDEN, "当前用户不是该节点审批人").into_response();
    }
    let store = ModelStore::new(pool);
    if let Err(err) = store.ensure(&model_def).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    match flow_store
        .add_sign(&flow.name, &model_name, record_id, &req.node, Some(user.id))
        .await
    {
        Ok(id) => Json(json!({ "instanceId": id, "currentNode": req.node, "status": "running" }))
            .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct AddSignReq {
    node: String,
}

fn flow_model_in_ir(state: &AppState, flow_name: &str) -> Option<String> {
    let build = state.build.read().expect("读取状态失败");
    build
        .ir
        .flows
        .iter()
        .find(|f| f.name == flow_name)
        .and_then(|f| f.trigger.as_deref().and_then(|t| t.split('.').next()))
        .map(str::to_string)
}

/// 流程实例状态：`GET /__kenvoy/flow/{flow}/records/{record_id}`。
async fn get_flow_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((flow_name, record_id)): AxumPath<(String, i64)>,
) -> Response {
    let Some(pool) = &state.db else {
        return (StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response();
    };
    if let Err(r) = auth_user(&state, &headers).await {
        return r;
    }
    let model_name = {
        let build = state.build.read().expect("读取状态失败");
        let Some(flow) = build.ir.flows.iter().find(|f| f.name == flow_name) else {
            return (StatusCode::NOT_FOUND, "流程不存在").into_response();
        };
        flow_model(flow)
            .map(str::to_string)
            .unwrap_or(flow_name.clone())
    };
    match FlowStore::new(pool)
        .latest_by_record(&model_name, record_id)
        .await
    {
        Ok(Some(instance)) => Json(json!({ "instance": instance })).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "流程未启动").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// 依据 Bearer token 解析当前用户与其角色名。
#[allow(clippy::result_large_err)] // 返回 `Response` 作为错误，便于直接转发
async fn auth_user(state: &AppState, headers: &HeaderMap) -> Result<(User, Vec<String>), Response> {
    let Some(pool) = &state.db else {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "数据库未配置").into_response());
    };
    let Some(token) = crate::auth::bearer_token(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "缺少 Bearer token").into_response());
    };
    let store = UserStore::new(pool);
    let Some(user) = store
        .user_by_session(token)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?
    else {
        return Err((StatusCode::UNAUTHORIZED, "会话无效或已过期").into_response());
    };
    let role_names = store
        .user_roles(user.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())?
        .into_iter()
        .map(|r| r.name)
        .collect();
    Ok((user, role_names))
}

/// 数据过滤决策。
enum FilterDecision {
    /// 全量可见。
    AllowAll,
    /// 无数据访问权限。
    Forbidden,
    /// OR 分组的过滤条件（组内 AND，如 `DataFilter` 多角色命中）。
    Conditions(Vec<Vec<FilterCond>>),
}

/// 按当前用户角色解析模型的 `DataFilter`。
///
/// 无规则 → 全量；无匹配角色 → 禁止；任一匹配为 `<All/>` → 全量；
/// 否则所有匹配角色的条件按 OR 合并（组内 AND）。
fn resolve_data_filter(
    ir: &ProjectIr,
    model: &str,
    roles: &[String],
    user: &User,
) -> FilterDecision {
    let mut filter_rules: Vec<&DataFilterRule> = Vec::new();
    for page in &ir.pages {
        if let Some(publish) = &page.publish {
            filter_rules.extend(publish.data_filters.iter().filter(|r| r.model == model));
        }
    }
    if filter_rules.is_empty() {
        return FilterDecision::AllowAll;
    }
    let matched: Vec<&RoleFilter> = filter_rules
        .iter()
        .flat_map(|r| &r.filters)
        .filter(|f| f.roles.iter().any(|r| roles.iter().any(|ur| ur == r)))
        .collect();
    if matched.is_empty() {
        return FilterDecision::Forbidden;
    }
    if matched.iter().any(|f| f.all) {
        return FilterDecision::AllowAll;
    }
    let ctx = user_context(user);
    let groups = matched
        .iter()
        .map(|f| {
            f.conditions
                .iter()
                .map(|c| FilterCond {
                    field: c.field.clone(),
                    op: c.op,
                    value: resolve_binding(&c.value, &ctx),
                })
                .collect()
        })
        .collect();
    FilterDecision::Conditions(groups)
}

/// 字段读权限：任一相关 `publish` 显式限制且当前角色不在 `read_roles` → 不可见。
fn field_read_visible(ir: &ProjectIr, field: &str, roles: &[String]) -> bool {
    for page in &ir.pages {
        if let Some(publish) = &page.publish {
            let Some(access) = publish.fields.iter().find(|f| f.field == field) else {
                continue;
            };
            if !access.read_roles.is_empty()
                && !access
                    .read_roles
                    .iter()
                    .any(|r| roles.iter().any(|ur| ur == r))
            {
                return false;
            }
        }
    }
    true
}

/// 数据 API 响应按当前角色裁剪无读权限的字段（保留 `id` / `created_at`）。
fn apply_field_read_perms(
    rows: Vec<Value>,
    model_def: &kengine::ir::Model,
    ir: &ProjectIr,
    roles: &[String],
) -> Vec<Value> {
    let visible: Vec<String> = model_def
        .fields
        .iter()
        .map(|f| f.name.clone())
        .filter(|f| field_read_visible(ir, f, roles))
        .collect();
    rows.into_iter()
        .map(|mut row| {
            if let Value::Object(map) = &mut row {
                map.retain(|k, _| k == "id" || k == "created_at" || visible.iter().any(|v| v == k));
            }
            row
        })
        .collect()
}

/// 字段写权限：任一相关 `publish` 显式限制且当前角色不在 `write_roles` → 禁止。
fn field_write_allowed(ir: &ProjectIr, field: &str, roles: &[String]) -> bool {
    for page in &ir.pages {
        if let Some(publish) = &page.publish {
            let Some(access) = publish.fields.iter().find(|f| f.field == field) else {
                continue;
            };
            if !access.write_roles.is_empty()
                && !access
                    .write_roles
                    .iter()
                    .any(|r| roles.iter().any(|ur| ur == r))
            {
                return false;
            }
        }
    }
    true
}

/// 构造表达式上下文（`currentUser.*`）。
fn user_context(user: &User) -> kengine::expr::Context {
    let mut ctx = kengine::expr::Context::new();
    ctx.insert(
        "currentUser.id",
        kengine::expr::Value::String(user.id.to_string()),
    );
    ctx.insert(
        "currentUser.username",
        kengine::expr::Value::String(user.username.clone()),
    );
    ctx.insert(
        "currentUser.displayName",
        kengine::expr::Value::String(user.display_name.clone()),
    );
    if let Some(email) = &user.email {
        ctx.insert(
            "currentUser.email",
            kengine::expr::Value::String(email.clone()),
        );
    }
    ctx
}

/// 解析 `{{ }}` 绑定；失败时原样返回。
fn resolve_binding(value: &str, ctx: &kengine::expr::Context) -> String {
    kengine::expr::resolve_bindings(value, ctx).unwrap_or_else(|_| value.to_string())
}

/// `serde_json` 值 → 表达式引擎值。
fn json_to_expr_value(v: &Value) -> kengine::expr::Value {
    match v {
        Value::Null => kengine::expr::Value::Null,
        Value::Bool(b) => kengine::expr::Value::Bool(*b),
        Value::Number(n) => kengine::expr::Value::Number(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => kengine::expr::Value::String(s.clone()),
        Value::Array(a) => kengine::expr::Value::Array(a.iter().map(json_to_expr_value).collect()),
        Value::Object(o) => kengine::expr::Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_expr_value(v)))
                .collect(),
        ),
    }
}

/// 收集页面树中引用指定模型的所有表单校验规则（按失败提示去重）。
fn collect_validations(pages: &[Page], model_name: &str) -> Vec<Validation> {
    let mut out = Vec::new();
    for page in pages {
        collect_node_validations(&page.content, model_name, &mut out);
    }
    out.sort_by_key(|v| v.message.clone());
    out.dedup_by_key(|v| v.message.clone());
    out
}

fn collect_node_validations(node: &Node, model_name: &str, out: &mut Vec<Validation>) {
    match node {
        Node::Form { props, children } => {
            if props.model == model_name {
                out.extend(props.validations.iter().cloned());
            }
            for child in children {
                collect_node_validations(child, model_name, out);
            }
        }
        Node::Row { children, .. }
        | Node::Col { children, .. }
        | Node::Section { children, .. }
        | Node::Custom { children, .. } => {
            for child in children {
                collect_node_validations(child, model_name, out);
            }
        }
        Node::Slot
        | Node::Field { .. }
        | Node::Table { .. }
        | Node::Column { .. }
        | Node::StatCard { .. }
        | Node::Text { .. }
        | Node::Chart { .. }
        | Node::FlowViewer { .. } => {}
    }
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
    use std::collections::BTreeMap;

    fn example_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/leave-app")
    }

    #[test]
    fn builds_example_project() {
        let state = build(&example_dir(), "测试", None).expect("构建示例项目失败");
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
        // 收集 `apps/` / `module/` / `I8n/` / `packages/` 下的 XML
        assert!(files.iter().all(|f| {
            f.path.starts_with("apps/")
                || f.path.starts_with("I8n/")
                || f.path.starts_with("module/")
                || f.path.starts_with("packages/")
        }));
        assert!(files.iter().any(|f| f.path == "I8n/zh/strings.kenvoy.xml"));
    }

    #[test]
    fn inject_dev_scripts_appends_scripts() {
        let html = "<html><body>hi</body></html>";
        let out = inject_dev_scripts(html, 3);
        assert!(out.contains("/__kenvoy/runtime.js"));
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

        let build_state = Arc::new(RwLock::new(build(&dir, "测试", None).expect("构建失败")));
        spawn_watcher(dir.clone(), "测试".to_string(), None, build_state.clone());

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
    #[allow(clippy::too_many_lines)] // 端到端路由验收用例，逐段断言可读性优先
    async fn dev_server_serves_pages_and_static() {
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tower::ServiceExt;

        let dir = example_dir();
        let build_state = Arc::new(RwLock::new(build(&dir, "测试", None).expect("构建失败")));
        let state = AppState {
            dir: dir.clone(),
            description: None,
            build: build_state,
            db: None,
            store: None,
            storage_cfg: StorageConfig::default(),
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let app = Router::new()
            .route("/", get(index))
            .route("/__kenvoy/version", get(version))
            .route("/__kenvoy/runtime.js", get(runtime_js))
            .route("/assets/tabler.min.css", get(tabler_css))
            .route("/__kenvoy/data/{model}", get(list_data).post(create_data))
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

        // 数据 API：创建 + 列表
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/__kenvoy/data/请假单")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"请假类型":"年假","请假天数":2}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/__kenvoy/data/请假单")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = String::from_utf8(
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("年假"));
    }

    #[test]
    fn data_filter_merges_multi_role_groups() {
        // 两个角色规则均命中 → OR 合并为两组条件（组内 AND）
        let ir = ProjectIr {
            version: kengine::ir::IR_VERSION.to_string(),
            project: kengine::ir::ProjectMeta {
                name: "t".into(),
                description: None,
                locale: None,
            },
            models: BTreeMap::new(),
            layouts: BTreeMap::new(),
            pages: vec![Page {
                path: "/x".into(),
                title: None,
                layout_chain: vec![],
                content: Node::Text {
                    props: kengine::ir::TextProps {
                        value: "x".into(),
                        i18n: None,
                    },
                },
                flows: vec![],
                publish: Some(kengine::ir::Publish {
                    allow_roles: vec![],
                    fields: vec![],
                    roles: vec![],
                    data_filters: vec![DataFilterRule {
                        model: "请假单".into(),
                        filters: vec![
                            RoleFilter {
                                roles: vec!["员工A".into()],
                                all: false,
                                conditions: vec![kengine::ir::OpCondition {
                                    field: "部门".into(),
                                    op: kengine::ir::FilterOp::Eq,
                                    value: "技术部".into(),
                                }],
                            },
                            RoleFilter {
                                roles: vec!["员工B".into()],
                                all: false,
                                conditions: vec![kengine::ir::OpCondition {
                                    field: "部门".into(),
                                    op: kengine::ir::FilterOp::Gt,
                                    value: "10".into(),
                                }],
                            },
                        ],
                    }],
                }),
            }],
            flows: vec![],
            permissions: kengine::ir::Permissions::default(),
            components: BTreeMap::new(),
            translations: BTreeMap::new(),
        };
        let user = User {
            id: 1,
            username: "a".into(),
            display_name: "A".into(),
            email: None,
            phone: None,
            status: "active".into(),
        };
        let decision = resolve_data_filter(
            &ir,
            "请假单",
            &["员工A".to_string(), "员工B".to_string()],
            &user,
        );
        match decision {
            FilterDecision::Conditions(groups) => {
                assert_eq!(groups.len(), 2, "多角色命中应按 OR 合并");
                assert_eq!(groups[0][0].op, kengine::ir::FilterOp::Eq);
                assert_eq!(groups[1][0].op, kengine::ir::FilterOp::Gt);
            }
            _ => panic!("期望 OR 条件组"),
        }
    }

    #[test]
    fn type_allowed_matches_wildcard() {
        assert!(type_allowed("image/png", "image/*"));
        assert!(type_allowed("image/jpeg", "image/*"));
        assert!(!type_allowed("application/pdf", "image/*"));
        assert!(type_allowed("application/pdf", "application/pdf"));
        assert!(!type_allowed("text/plain", "application/pdf"));
    }

    #[test]
    fn field_read_perm_filters_by_role() {
        let ir = ProjectIr {
            version: kengine::ir::IR_VERSION.to_string(),
            project: kengine::ir::ProjectMeta {
                name: "t".into(),
                description: None,
                locale: None,
            },
            models: BTreeMap::new(),
            layouts: BTreeMap::new(),
            pages: vec![Page {
                path: "/x".into(),
                title: None,
                layout_chain: vec![],
                content: Node::Text {
                    props: kengine::ir::TextProps {
                        value: "x".into(),
                        i18n: None,
                    },
                },
                flows: vec![],
                publish: Some(kengine::ir::Publish {
                    allow_roles: vec![],
                    fields: vec![kengine::ir::FieldAccess {
                        field: "薪资".into(),
                        read_roles: vec!["管理员".into()],
                        write_roles: vec![],
                    }],
                    roles: vec![],
                    data_filters: vec![],
                }),
            }],
            flows: vec![],
            permissions: kengine::ir::Permissions::default(),
            components: BTreeMap::new(),
            translations: BTreeMap::new(),
        };
        assert!(field_read_visible(&ir, "薪资", &["管理员".to_string()]));
        assert!(!field_read_visible(&ir, "薪资", &["普通员工".to_string()]));
        // 未声明的字段不受限
        assert!(field_read_visible(&ir, "姓名", &["普通员工".to_string()]));
    }

    #[tokio::test]
    async fn upload_without_storage_returns_503() {
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tower::ServiceExt;

        let dir = example_dir();
        let build_state = Arc::new(RwLock::new(build(&dir, "测试", None).expect("构建失败")));
        let state = AppState {
            dir: dir.clone(),
            description: None,
            build: build_state,
            db: None,
            store: None,
            storage_cfg: StorageConfig::default(),
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };
        let app = Router::new()
            .route("/__kenvoy/file", post(upload_file))
            .with_state(state);

        let body = "----kv\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"t.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            hello\r\n\
            ----kv--\r\n";
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/__kenvoy/file")
                    .header("content-type", "multipart/form-data; boundary=kv")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        let _ = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    }
}
