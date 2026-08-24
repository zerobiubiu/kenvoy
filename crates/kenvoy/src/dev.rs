//! `dev` 开发服务器：扫描 `apps/` → kengine 解析 → kengine-html 渲染 → 浏览器预览。
//!
//! 按 D3/D4 决策：axum + notify + tokio；文件变更后整页刷新（轮询自动刷新，非 SSE/WS）。

use std::fmt::Write;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, mpsc};

use anyhow::Context;
use axum::Router;
use axum::extract::{Json, Multipart, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use kengine::ir::{DataFilterRule, Node, Page, ProjectIr, RoleFilter, Validation};
use kengine::validation;
use kengine::{SourceFile, build_project};
use kengine_db::users::User;
use kengine_db::{ModelStore, StorageStore, UserStore};
use kengine_html::render_page;
use kengine_s3::ObjectStore;
use notify::Watcher;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::config::StorageConfig;

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
    /// 内存数据存储（模型名 → 记录列表，无数据库时的降级模式）。
    data: Arc<Mutex<std::collections::HashMap<String, Vec<Value>>>>,
}

/// 启动开发服务器。
pub async fn run(
    dir: &Path,
    host: &str,
    port: u16,
    project_name: &str,
    description: Option<String>,
    storage: &StorageConfig,
    db: Option<PgPool>,
) -> anyhow::Result<()> {
    let build_state = Arc::new(RwLock::new(
        build(dir, project_name).with_context(|| format!("构建项目失败：{}", dir.display()))?,
    ));
    spawn_watcher(
        dir.to_path_buf(),
        project_name.to_string(),
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
        data: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/__kenvoy/version", get(version))
        .route("/__kenvoy/runtime.js", get(runtime_js))
        .route("/assets/tabler.min.css", get(tabler_css))
        .route("/__kenvoy/data/{model}", get(list_data).post(create_data))
        .route("/__kenvoy/file", post(upload_file))
        .route("/__kenvoy/file/{id}", get(download_file))
        .nest(
            "/__kenvoy/auth",
            crate::auth::router().with_state(crate::auth::AuthState { db: db_for_auth }),
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
pub(crate) fn build_ir(dir: &Path, project_name: &str) -> anyhow::Result<ProjectIr> {
    let mut files = Vec::new();
    collect_source_files(dir, &mut files)?;
    Ok(build_project(&files, project_name)?)
}

/// 扫描 `apps/` 并构建 `IR`。
fn build(dir: &Path, project_name: &str) -> anyhow::Result<BuildState> {
    let ir = build_ir(dir, project_name)?;
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
fn spawn_watcher(dir: PathBuf, project_name: String, state: Arc<RwLock<BuildState>>) {
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
            rebuild(&dir, &project_name, &state);
        }
    });
}

/// 重建 `IR`：成功则替换并递增版本；失败则保留旧版本并打印错误。
fn rebuild(dir: &Path, project_name: &str, state: &RwLock<BuildState>) {
    let next_version = state
        .read()
        .expect("读取状态失败")
        .version
        .saturating_add(1);
    match build(dir, project_name) {
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
        kengine_html::RUNTIME_JS,
    )
        .into_response()
}

/// 返回 Tabler 样式。
async fn tabler_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        kengine_html::TABLER_CSS,
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

/// `[storage]` 配置齐全时转换为 `kengine_s3::S3Config`，否则返回 `None`。
fn s3_config(cfg: &StorageConfig) -> Option<kengine_s3::S3Config> {
    let s = cfg;
    Some(kengine_s3::S3Config {
        endpoint: s.endpoint.clone()?,
        region: s.region.clone()?,
        bucket: s.bucket.clone()?,
        access_key: s.access_key.clone()?,
        secret_key: s.secret_key.clone()?,
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
/// 启用数据库时要求登录（Bearer token），并按 `DataFilter` 做行级过滤。
async fn list_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(model): AxumPath<String>,
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
        let conditions = {
            let build = state.build.read().expect("读取状态失败");
            match resolve_data_filter(&build.ir, &model, &roles, &user) {
                FilterDecision::AllowAll => Vec::new(),
                FilterDecision::Forbidden => {
                    return (StatusCode::FORBIDDEN, "无数据访问权限").into_response();
                }
                FilterDecision::Conditions(c) => c,
            }
        };
        match store.list_where(&model_def, &conditions).await {
            Ok(rows) => Json(rows).into_response(),
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
    /// AND 相等的过滤条件。
    Conditions(Vec<(String, String)>),
}

/// 按当前用户角色解析模型的 `DataFilter`。
///
/// 无规则 → 全量；无匹配角色 → 禁止；任一匹配为 `<All/>` → 全量；否则取首个匹配条件。
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
    let conditions = matched[0]
        .conditions
        .iter()
        .map(|c| (c.field.clone(), resolve_binding(&c.value, &ctx)))
        .collect();
    FilterDecision::Conditions(conditions)
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
        | Node::Text { .. } => {}
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

    fn example_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/leave-app")
    }

    #[test]
    fn builds_example_project() {
        let state = build(&example_dir(), "测试").expect("构建示例项目失败");
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

        let build_state = Arc::new(RwLock::new(build(&dir, "测试").expect("构建失败")));
        spawn_watcher(dir.clone(), "测试".to_string(), build_state.clone());

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
        let build_state = Arc::new(RwLock::new(build(&dir, "测试").expect("构建失败")));
        let state = AppState {
            dir: dir.clone(),
            description: None,
            build: build_state,
            db: None,
            store: None,
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

    #[tokio::test]
    async fn upload_without_storage_returns_503() {
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tower::ServiceExt;

        let dir = example_dir();
        let build_state = Arc::new(RwLock::new(build(&dir, "测试").expect("构建失败")));
        let state = AppState {
            dir: dir.clone(),
            description: None,
            build: build_state,
            db: None,
            store: None,
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
