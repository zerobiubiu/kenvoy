use super::*;
use axum::{body::Bytes, http::Request};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
const SOURCE: &str = r#"export default <app name="客户应用"><resource name="customers"><text name="name" required maxLength={100}/><text name="phone"/><number name="age" integer min={0}/></resource><page path="/customers"><form resource="customers"/></page><page path="/private" permission="admin"><table resource="customers"/></page></app>;"#;
const PASSWORD: &str = "测试密码-123456789";

#[tokio::test]
async fn first_navigation_waits_for_publication_without_holding_model_lock() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    f.runtime.enable_development(None).await?;
    f.runtime
        .set_development_status(DevelopmentPhase::Building, None, false)
        .await?;
    let timed_out = super::development::wait_ready(&f.runtime.shared, Duration::from_millis(20))
        .await
        .expect_err("构建未完成应限时返回")
        .into_response();
    assert_eq!(timed_out.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(timed_out.headers()[header::RETRY_AFTER], "1");
    let runtime = f.runtime.clone();
    let input = Request::builder()
        .uri("/customers")
        .header("host", "127.0.0.1:3000")
        .body(Body::empty())?;
    let request = tokio::spawn(async move { runtime.router().oneshot(input).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!request.is_finished());
    let mut next = f.app.clone();
    next.pages
        .iter_mut()
        .find(|page| page.path == "/customers")
        .ok_or("缺少页面")?
        .permission = Role::Admin;
    tokio::time::timeout(
        Duration::from_secs(2),
        f.runtime.publish_prepared(next, None, None),
    )
    .await??;
    f.runtime
        .set_development_status(DevelopmentPhase::Ready, None, true)
        .await?;
    let response = tokio::time::timeout(Duration::from_secs(2), request).await???;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn development_proxy_keeps_origin_secrets_routes_and_previous_generation_separate()
-> TestResult {
    use axum::routing::any;
    type Captured = Arc<Mutex<Vec<(String, HeaderMap)>>>;
    async fn upstream(
        State(captured): State<Captured>,
        headers: HeaderMap,
        uri: axum::http::Uri,
    ) -> Response {
        captured
            .lock()
            .expect("测试捕获锁")
            .push((uri.to_string(), headers));
        match uri.path() {
            "/_kenvoy/vite/index.html" | "/_kenvoy/vite/unknown" => (
                [(header::CONTENT_TYPE, "text/html")],
                "<html><script type='module' src='/_kenvoy/vite/preamble.js'></script></html>",
            )
                .into_response(),
            "/_kenvoy/vite/redirect" => (
                StatusCode::FOUND,
                [(header::LOCATION, "http://untrusted.invalid/private")],
            )
                .into_response(),
            _ => (
                [
                    (header::CONTENT_TYPE, "text/javascript"),
                    (header::SET_COOKIE, "stolen=1"),
                    (header::LOCATION, "http://untrusted.invalid"),
                ],
                "export const value=42;",
            )
                .into_response(),
        }
    }
    let captured = Captured::default();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel();
    let service = Router::new()
        .fallback(any(upstream))
        .with_state(captured.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let f = Fixture::new(SOURCE).await?;
    for path in [
        "/_kenvoy/development",
        "/_kenvoy/vite/hmr",
        "/_kenvoy/vite/source.js",
    ] {
        assert_eq!(
            f.request("GET", path, &[], Body::empty()).await?.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            f.request("POST", path, &[], Body::empty()).await?.0,
            StatusCode::NOT_FOUND
        );
    }
    assert!(
        f.runtime
            .enable_development(Some(DevelopmentProxy {
                endpoint: "192.0.2.1:80".parse()?,
                secret: "a".repeat(64)
            }))
            .await
            .is_err()
    );
    f.runtime
        .enable_development(Some(DevelopmentProxy {
            endpoint,
            secret: "a".repeat(64),
        }))
        .await?;
    let metadata = f
        .request("GET", "/_kenvoy/application", &[], Body::empty())
        .await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&metadata.2)?["development"],
        true
    );
    let initial = serde_json::from_slice::<Value>(
        &f.request("GET", "/_kenvoy/development", &[], Body::empty())
            .await?
            .2,
    )?;
    assert_eq!(initial["revision"], 1);
    assert_eq!(initial["state"], "ready");
    f.runtime
        .set_development_status(DevelopmentPhase::Building, None, false)
        .await?;
    f.runtime
        .set_development_status(
            DevelopmentPhase::Error,
            Some("声明校验失败，请检查终端诊断".into()),
            false,
        )
        .await?;
    let failed = serde_json::from_slice::<Value>(
        &f.request("GET", "/_kenvoy/development", &[], Body::empty())
            .await?
            .2,
    )?;
    assert_eq!(failed["revision"], 3);
    assert_eq!(failed["generation"], initial["generation"]);
    let cookie = format!("kenvoy_session={}", "0".repeat(64));
    let response = f
        .request(
            "GET",
            "/_kenvoy/vite/source.js?v=1",
            &[
                ("cookie", &cookie),
                ("authorization", "Bearer hidden"),
                ("x-kenvoy-csrf", "hidden"),
                ("x-kenvoy-vite-secret", "spoof"),
            ],
            Body::empty(),
        )
        .await?;
    assert_eq!(response.0, StatusCode::OK);
    assert!(!response.1.contains_key(header::SET_COOKIE));
    assert!(!response.1.contains_key(header::LOCATION));
    assert!(String::from_utf8_lossy(&response.2).contains("value=42"));
    {
        let seen = captured.lock().map_err(|_| "捕获锁损坏")?;
        let (uri, headers) = seen.last().ok_or("上游未接收")?;
        assert_eq!(uri, "/_kenvoy/vite/source.js?v=1");
        assert_eq!(headers["x-kenvoy-vite-secret"], "a".repeat(64));
        for name in ["cookie", "authorization", "x-kenvoy-csrf", "origin"] {
            assert!(!headers.contains_key(name));
        }
        assert_ne!(headers["host"], "127.0.0.1:3000");
    }
    assert_eq!(
        f.request("GET", "/customers", &[], Body::empty()).await?.0,
        StatusCode::OK
    );
    let before = captured.lock().map_err(|_| "捕获锁损坏")?.len();
    assert_eq!(
        f.request("GET", "/private", &[], Body::empty()).await?.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request("GET", "/unregistered", &[], Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(captured.lock().map_err(|_| "捕获锁损坏")?.len(), before);
    for path in [
        "/_kenvoy/vite/index.html",
        "/_kenvoy/vite/%2e%2e/secret",
        "/_kenvoy/vite/%2Fsecret",
        "/_kenvoy/vite/%5csecret",
        "/_kenvoy/vite/%00",
        "/_kenvoy/vite/%FF",
        "/_kenvoy/vite/unknown",
    ] {
        assert_eq!(
            f.request("GET", path, &[], Body::empty()).await?.0,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    assert_eq!(
        f.request("GET", "/_kenvoy/vite/redirect", &[], Body::empty())
            .await?
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        f.request(
            "GET",
            "/_kenvoy/vite/source.js",
            &[("sec-fetch-site", "cross-site")],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    f.runtime.replace_development_proxy(None).await?;
    assert_eq!(
        f.request("GET", "/_kenvoy/vite/source.js", &[], Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    f.database.shutdown().await?;
    let _ = stop.send(());
    server.await??;
    Ok(())
}

#[tokio::test]
async fn development_websocket_uses_fixed_loopback_protocol_and_closes_on_shutdown() -> TestResult {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};
    async fn upstream(headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
        if headers
            .get("x-kenvoy-vite-secret")
            .and_then(|value| value.to_str().ok())
            != Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            || headers.contains_key(header::COOKIE)
            || headers.contains_key(header::AUTHORIZATION)
        {
            return StatusCode::FORBIDDEN.into_response();
        }
        ws.protocols(["vite-hmr"])
            .on_upgrade(|mut socket| async move {
                if socket.send(Message::Text("ready".into())).await.is_err() {
                    return;
                }
                while let Some(Ok(message)) = socket.next().await {
                    if matches!(message, Message::Close(_)) {
                        break;
                    }
                    if socket.send(message).await.is_err() {
                        break;
                    }
                }
            })
    }
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = listener.local_addr()?;
    let (stop_upstream, upstream_stopped) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/_kenvoy/vite/hmr", get(upstream)),
        )
        .with_graceful_shutdown(async {
            let _ = upstream_stopped.await;
        })
        .await
    });
    let f = Fixture::new(SOURCE).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let origin = format!("http://{address}");
    let runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        origin.clone(),
        None,
    )
    .await?;
    runtime
        .enable_development(Some(DevelopmentProxy {
            endpoint,
            secret: "a".repeat(64),
        }))
        .await?;
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        runtime
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
    });
    let url = format!("ws://{address}/_kenvoy/vite/hmr?token=test");
    for (request_origin, protocol) in [
        ("http://evil.invalid", "vite-hmr"),
        (origin.as_str(), "other"),
    ] {
        let mut request = url.clone().into_client_request()?;
        request
            .headers_mut()
            .insert(header::ORIGIN, request_origin.parse()?);
        request
            .headers_mut()
            .insert(header::SEC_WEBSOCKET_PROTOCOL, protocol.parse()?);
        let error = tokio_tungstenite::connect_async(request)
            .await
            .expect_err("非法握手必须失败");
        assert!(
            matches!(error,tungstenite::Error::Http(response) if response.status()==StatusCode::FORBIDDEN)
        );
    }
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert(header::ORIGIN, origin.parse()?);
    request.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static("vite-hmr"),
    );
    request.headers_mut().insert(
        header::COOKIE,
        format!("kenvoy_session={}", "0".repeat(64)).parse()?,
    );
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer private"),
    );
    let (mut socket, response) = tokio_tungstenite::connect_async(request).await?;
    assert_eq!(
        response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "vite-hmr"
    );
    assert_eq!(
        socket.next().await.ok_or("WS提前关闭")??.to_text()?,
        "ready"
    );
    socket
        .send(tungstenite::Message::Text("update".into()))
        .await?;
    assert_eq!(
        socket.next().await.ok_or("WS提前关闭")??.to_text()?,
        "update"
    );
    let _ = stop.send(());
    let message = tokio::time::timeout(Duration::from_secs(3), socket.next()).await?;
    assert!(message.is_none() || matches!(message, Some(Ok(tungstenite::Message::Close(_)))));
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    let _ = stop_upstream.send(());
    upstream_server.await??;
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn unicode_page_form_ids_use_one_strict_percent_decoding_pass() -> TestResult {
    let source = SOURCE.replace("/customers", "/客户");
    let mut f = Fixture::new(&source).await?;
    let client = f.root.join("client");
    fs::create_dir(&client)?;
    fs::write(
        client.join("index.html"),
        "<!doctype html><div id='unicode-page'></div>",
    )?;
    f.runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "http://127.0.0.1:3000".into(),
        Some(client.clone()),
    )
    .await?;
    for path in [
        "/%25E5%25AE%25A2%25E6%2588%25B7",
        "/%2f%E5%AE%A2%E6%88%B7",
        "/%5C%E5%AE%A2%E6%88%B7",
        "/%00",
        "/%FF",
        "/%ZZ",
    ] {
        assert_eq!(
            f.request("GET", path, &[], Body::empty()).await?.0,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        format!("http://{address}"),
        Some(client),
    )
    .await?;
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        runtime
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
    });
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(
            format!(
                "GET /%E5%AE%A2%E6%88%B7 HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    let mut response = vec![];
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response)).await??;
    let response = String::from_utf8(response)?;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("id='unicode-page'"));
    stop.send(()).map_err(|_| "服务已提前关闭")?;
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    let (cookie, csrf) = f.context().await?;
    let mut headers = vec![
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
        ("x-kenvoy-form", "%2F%E5%AE%A2%E6%88%B7%23form-1"),
    ];
    assert_eq!(
        f.request("POST", "/api/customers", &headers, r#"{"name":"中文路由"}"#)
            .await?
            .0,
        StatusCode::CREATED
    );
    *headers.last_mut().ok_or("缺少表单头")? = ("x-kenvoy-form", "%FF");
    assert_eq!(
        f.request("POST", "/api/customers", &headers, r#"{"name":"非法编码"}"#)
            .await?
            .0,
        StatusCode::BAD_REQUEST
    );
    *headers.last_mut().ok_or("缺少表单头")? =
        ("x-kenvoy-form", "%252F%E5%AE%A2%E6%88%B7%23form-1");
    assert_eq!(
        f.request("POST", "/api/customers", &headers, r#"{"name":"双重编码"}"#)
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.database
            .list("customers", Default::default())
            .await?
            .total,
        1
    );
    f.database.shutdown().await?;
    Ok(())
}

struct Fixture {
    root: PathBuf,
    app: ApplicationIR,
    database: Database,
    auth: Auth,
    runtime: HttpRuntime,
}
impl Fixture {
    async fn new(source: &str) -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "kenvoy-http-{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root)?;
        let entry = root.join("app.tsx");
        fs::write(&entry, source)?;
        let program = kengine::Engine::new().analyze_project(
            &root,
            kengine::AnalyzeOptions {
                entry_points: vec![entry.clone()],
                ..Default::default()
            },
        )?;
        let result = crate::dsl::analyze(&program, &entry.canonicalize()?, "测试");
        let app = result
            .application
            .ok_or_else(|| format!("{:?}", result.diagnostics))?;
        let database = Database::open(root.join("app.sqlite"), app.clone()).await?;
        let auth = Auth::new(database.clone()).await?;
        auth.init_admin("admin", PASSWORD).await?;
        let runtime = HttpRuntime::new(
            app.clone(),
            database.clone(),
            auth.clone(),
            "http://127.0.0.1:3000".into(),
            None,
        )
        .await?;
        Ok(Self {
            root,
            app,
            database,
            auth,
            runtime,
        })
    }
    async fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: impl Into<Body>,
    ) -> TestResult<(StatusCode, HeaderMap, Bytes)> {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "127.0.0.1:3000");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = self
            .runtime
            .router()
            .oneshot(request.body(body.into())?)
            .await?;
        let (parts, body) = response.into_parts();
        Ok((
            parts.status,
            parts.headers,
            to_bytes(body, 2 * BODY_LIMIT).await?,
        ))
    }
    async fn context(&self) -> TestResult<(String, String)> {
        let (status, headers, body) = self
            .request("GET", "/_kenvoy/session", &[], Body::empty())
            .await?;
        assert_eq!(status, StatusCode::OK);
        let cookie = headers
            .get(header::SET_COOKIE)
            .ok_or("缺少 Cookie")?
            .to_str()?
            .split(';')
            .next()
            .ok_or("Cookie 无效")?
            .to_owned();
        let body: Value = serde_json::from_slice(&body)?;
        Ok((
            cookie,
            body["csrf_token"].as_str().ok_or("缺少 CSRF")?.into(),
        ))
    }
    async fn login(&self) -> TestResult<(String, String)> {
        self.login_as("admin").await
    }
    async fn login_as(&self, username: &str) -> TestResult<(String, String)> {
        let (cookie, csrf) = self.context().await?;
        let (status, headers, body) = self
            .request(
                "POST",
                "/_kenvoy/login",
                &[
                    ("cookie", &cookie),
                    ("origin", "http://127.0.0.1:3000"),
                    ("x-kenvoy-csrf", &csrf),
                    ("content-type", "application/json"),
                ],
                json!({"username":username,"password":PASSWORD}).to_string(),
            )
            .await?;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let next = headers
            .get(header::SET_COOKIE)
            .ok_or("缺少新 Cookie")?
            .to_str()?
            .split(';')
            .next()
            .ok_or("Cookie 无效")?
            .to_owned();
        assert_ne!(next, cookie);
        let body: Value = serde_json::from_slice(&body)?;
        Ok((next, body["csrf_token"].as_str().ok_or("缺少 CSRF")?.into()))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[tokio::test]
async fn server_actions_execute_compiled_esm_with_permissions_csrf_and_atomic_generations()
-> TestResult {
    use crate::javascript::{self, JavascriptRuntime, Limits, Trust};
    let mut f = Fixture::new("export default <page path='/actions'/>;").await?;
    fs::write(f.root.join("math.ts"), "export const factor:number=2;")?;
    fs::write(
        f.root.join("actions.server.ts"),
        "import {factor} from './math'; export async function sum(value:number){await Promise.resolve();return {result:value*factor};} export function secret(){return '仅管理员';} export function fail(){throw new Error('private-secret-path');} export function pending(){return new Promise(()=>{});}",
    )?;
    let source = "import {sum,secret,fail,pending} from './actions.server'; export default <app name='动作'><action name='sum' runtime='server' permission='anonymous' handler={sum}/><action name='secret' runtime='server' permission='admin' handler={secret}/><action name='fail' runtime='server' permission='anonymous' handler={fail}/><action name='pending' runtime='server' permission='anonymous' handler={pending}/><page path='/actions'/></app>;";
    fs::write(f.root.join("app.tsx"), source)?;
    let program = kengine::Engine::new().analyze_project(
        &f.root,
        kengine::AnalyzeOptions {
            entry_points: vec!["app.tsx".into()],
            ..Default::default()
        },
    )?;
    let app = crate::dsl::analyze(&program, &f.root.join("app.tsx").canonicalize()?, "测试")
        .application
        .ok_or("Action 分析失败")?;
    let bundle = javascript::prepare(&program, &app, Trust::Project)?;
    let js = JavascriptRuntime::start(
        Arc::new(bundle.clone()),
        Limits {
            timeout: Duration::from_millis(150),
            ..Limits::default()
        },
        Trust::Project,
    )?;
    f.database.replace_application(app.clone(), None).await?;
    f.runtime = HttpRuntime::new(
        app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "http://127.0.0.1:3000".into(),
        None,
    )
    .await?
    .with_javascript(js.clone())
    .await?;
    let (cookie, csrf) = f.context().await?;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
    ];
    let response = f
        .request("POST", "/api/actions/sum", &headers, r#"{"args":[21]}"#)
        .await?;
    assert_eq!(
        response.0,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response.2)
    );
    let result: Value = serde_json::from_slice(&response.2)?;
    assert_eq!(result["data"], json!({"result":42}));
    let old_generation = result["generation"].as_u64().ok_or("缺少代际")?;
    assert_eq!(
        f.request("POST", "/api/actions/secret", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request("POST", "/api/actions/unknown", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("GET", "/api/actions/sum", &[], Body::empty())
            .await?
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    for body in [
        r#"{"args":[],"extra":1}"#,
        r#"{"args":[],"args":[]}"#,
        r#"{"args":[{"value":1,"value":2}]}"#,
        r#"{"args":[9007199254740993]}"#,
    ] {
        assert_eq!(
            f.request("POST", "/api/actions/sum", &headers, body)
                .await?
                .0,
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    assert_eq!(
        f.request("POST", "/api/actions/sum", &headers[..2], r#"{"args":[1]}"#)
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let failed = f
        .request("POST", "/api/actions/fail", &headers, r#"{"args":[]}"#)
        .await?;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!String::from_utf8_lossy(&failed.2).contains("private-secret"));
    assert_eq!(
        f.request("POST", "/api/actions/pending", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::GATEWAY_TIMEOUT
    );
    let (admin_cookie, admin_csrf) = f.login().await?;
    let admin_headers = [
        ("cookie", admin_cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", admin_csrf.as_str()),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/actions/secret",
            &admin_headers,
            r#"{"args":[]}"#
        )
        .await?
        .0,
        StatusCode::OK
    );
    let mut updated = bundle;
    for module in updated.modules.values_mut() {
        module.code = module.code.replace("factor = 2", "factor = 3");
    }
    let updated_js = js.with_bundle(Arc::new(updated))?;
    let next = f
        .runtime
        .publish_prepared(app.clone(), None, Some(updated_js))
        .await?;
    assert_eq!(next, old_generation + 1);
    assert!(f.runtime.publish_prepared(app, None, None).await.is_err());
    let response = f
        .request("POST", "/api/actions/sum", &headers, r#"{"args":[14]}"#)
        .await?;
    assert_eq!(response.0, StatusCode::OK);
    let result: Value = serde_json::from_slice(&response.2)?;
    assert_eq!(result["data"], json!({"result":42}));
    assert_eq!(result["generation"], next);
    let mut stale = headers.to_vec();
    let stale_generation = old_generation.to_string();
    stale.push(("x-kenvoy-generation", stale_generation.as_str()));
    assert_eq!(
        f.request("POST", "/api/actions/sum", &stale, r#"{"args":[1]}"#)
            .await?
            .0,
        StatusCode::CONFLICT
    );
    js.shutdown().await?;
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn option_projection_is_form_scoped_and_never_grants_source_crud() -> TestResult {
    let source = r#"export default <app name='选项协议'><resource name='cities'><text name='code' unique required/><text name='name' required/><text name='province' required/><text name='secret'/></resource><resource name='customers'><text name='province'/><select name='city' optionsSource={{resource:'cities',value:'code',label:'name',filters:{province:'province'},permission:'anonymous'}}/><select name='city_id' optionsSource={{resource:'cities',value:'id',label:'name',permission:'anonymous'}}/><select name='private_city' optionsSource={{resource:'cities',value:'code',label:'name',permission:'admin'}}/></resource><page path='/customers'><form resource='customers'/></page><page path='/other'><form resource='customers' fields={['province']}/></page></app>;"#;
    let source = source.replace(
        "<form resource='customers'/>",
        "<form resource='customers' fields={['province','city','city_id']}/>",
    );
    let f = Fixture::new(&source).await?;
    let city = f
        .database
        .create(
            "cities",
            json!({"code":"hz","name":"杭州","province":"zj","secret":"不公开"}),
            None,
        )
        .await?;
    f.database
        .create(
            "cities",
            json!({"code":"nj","name":"南京","province":"js"}),
            None,
        )
        .await?;
    let (cookie, csrf) = f.context().await?;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/customers#form-1"),
        ("content-type", "application/json"),
    ];
    let path = "/api/customers/options/city";
    let input = json!({"dependencies":{"province":"zj"},"selected":"hz"}).to_string();
    let (status, _, body) = f.request("POST", path, &headers, input.clone()).await?;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let result: Value = serde_json::from_slice(&body)?;
    assert_eq!(result["data"], json!([{"value":"hz","label":"杭州"}]));
    assert_eq!(result["selected"], json!({"value":"hz","label":"杭州"}));
    assert!(!String::from_utf8_lossy(&body).contains("不公开"));
    assert_eq!(
        f.request("GET", "/api/cities", &headers, Body::empty())
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/customers/options/private_city",
            &headers,
            "{}"
        )
        .await?
        .0,
        StatusCode::UNAUTHORIZED
    );
    for body in [
        r#"{"dependencies":{"secret":"不公开"}}"#,
        r#"{"resource":"_kenvoy_users"}"#,
        r#"{"dependencies":{"province":"zj","province":"js"}}"#,
        r#"{"page_size":101}"#,
    ] {
        assert_eq!(
            f.request("POST", path, &headers, body).await?.0,
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    let mut wrong_form = headers.to_vec();
    wrong_form[3] = ("x-kenvoy-form", "/other#form-1");
    assert_eq!(
        f.request("POST", path, &wrong_form, input.clone()).await?.0,
        StatusCode::UNAUTHORIZED
    );
    let mut no_csrf = headers.to_vec();
    no_csrf.remove(2);
    assert_eq!(
        f.request("POST", path, &no_csrf, input.clone()).await?.0,
        StatusCode::FORBIDDEN
    );
    let mut stale = headers.to_vec();
    stale.push(("x-kenvoy-generation", "0"));
    assert_eq!(
        f.request("POST", path, &stale, input).await?.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.request("POST", "/api/customers/options/missing", &headers, "{}")
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    let id_result = f
        .request("POST", "/api/customers/options/city_id", &headers, "{}")
        .await?;
    let ids: Value = serde_json::from_slice(&id_result.2)?;
    let city_id = city["id"].to_string();
    assert!(
        ids["data"]
            .as_array()
            .ok_or("缺少选项")?
            .iter()
            .any(|value| value["value"].as_str() == Some(city_id.as_str()))
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/customers",
            &headers,
            r#"{"province":"js","city":"hz"}"#
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/customers",
            &headers,
            json!({"province":"zj","city":"hz","city_id":city["id"].to_string()}).to_string()
        )
        .await?
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        f.request("POST", "/api/customers", &headers, r#"{"city_id":"01"}"#)
            .await?
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (admin, csrf) = f.login().await?;
    let admin_headers = [
        ("cookie", admin.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/customers/options/private_city",
            &admin_headers,
            "{}"
        )
        .await?
        .0,
        StatusCode::OK
    );
    // 即使直接加载的产物绕过 DSL 检查，写入也不能用 201/422 区分私有候选是否存在。
    let mut restricted = f.app.clone();
    let city = restricted
        .resources
        .iter_mut()
        .find(|resource| resource.name == "customers")
        .and_then(|resource| {
            resource
                .fields
                .iter_mut()
                .find(|field| field.name == "city")
        })
        .ok_or("缺少城市字段")?;
    city.options_source.as_mut().ok_or("缺少来源")?.permission = Role::Admin;
    f.runtime.publish_prepared(restricted, None, None).await?;
    for candidate in ["hz", "missing"] {
        assert_eq!(
            f.request(
                "POST",
                "/api/customers",
                &headers,
                json!({"province":"zj","city":candidate}).to_string()
            )
            .await?
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn public_form_and_authenticated_crud_share_real_sqlite() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let (cookie, csrf) = f.context().await?;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/customers#form-1"),
        ("content-type", "application/json"),
    ];
    let (status, _, body) = f
        .request(
            "POST",
            "/api/customers",
            &headers,
            json!({"name":"张三","phone":"13800000000","age":25}).to_string(),
        )
        .await?;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let created: Value = serde_json::from_slice(&body)?;
    let id = created["data"]["id"].as_i64().ok_or("缺少id")?;
    assert_eq!(
        f.database.get("customers", id).await?.ok_or("未持久化")?["name"],
        "张三"
    );
    for path in ["/api/customers".to_owned(), format!("/api/customers/{id}")] {
        assert_eq!(
            f.request("GET", &path, &[("cookie", &cookie)], Body::empty())
                .await?
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (admin, csrf) = f.login().await?;
    let headers = [
        ("cookie", admin.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
    ];
    let (status, _, body) = f
        .request(
            "GET",
            "/api/customers?page=1&page_size=10&sort=age&direction=desc",
            &[("cookie", &admin)],
            Body::empty(),
        )
        .await?;
    assert_eq!(status, StatusCode::OK);
    let rows: Value = serde_json::from_slice(&body)?;
    assert_eq!(rows["total"], 1);
    assert_eq!(rows["data"][0]["name"], "张三");
    let path = format!("/api/customers/{id}");
    assert_eq!(
        f.request("PATCH", &path, &headers, r#"{"age":26}"#)
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.database.get("customers", id).await?.ok_or("缺少记录")?["age"],
        26
    );
    assert_eq!(
        f.request("DELETE", &path, &headers, Body::empty()).await?.0,
        StatusCode::NO_CONTENT
    );
    assert!(f.database.get("customers", id).await?.is_none());
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn metadata_and_public_response_hide_unexposed_fields_and_host_paths() -> TestResult {
    let source = r#"export default <app name="投影"><resource name="items"><text name="name"/><text name="phone"/><text name="secret" default="private-secret"/></resource><page path="/one"><form resource="items" fields={["name"]}/></page><page path="/two"><form resource="items" fields={["phone"]}/></page><page path="/private" permission="admin"><table resource="items"/></page></app>;"#;
    let f = Fixture::new(source).await?;
    let (status, _, body) = f
        .request("GET", "/_kenvoy/application", &[], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8(body.to_vec())?;
    assert!(!text.contains("private-secret"));
    assert!(!text.contains("app.tsx"));
    assert!(!text.contains("\"source\":"));
    assert!(!text.contains("/private"));
    let metadata: Value = serde_json::from_str(&text)?;
    assert_eq!(
        metadata["resources"][0]["fields"]
            .as_array()
            .ok_or("缺少字段")?
            .len(),
        2
    );
    let (cookie, csrf) = f.context().await?;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/one#form-1"),
        ("content-type", "application/json"),
    ];
    let (status, _, body) = f
        .request("POST", "/api/items", &headers, r#"{"name":"公开"}"#)
        .await?;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let record: Value = serde_json::from_slice(&body)?;
    assert!(record["data"].get("secret").is_none());
    assert!(record["data"].get("phone").is_none());
    assert_eq!(
        f.request(
            "POST",
            "/api/items",
            &headers,
            r#"{"name":"公开","phone":"越权"}"#
        )
        .await?
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.database.list("items", Default::default()).await?.total, 1);
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn request_boundaries_reject_csrf_pollution_oversize_and_unknown_routes() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let (admin, csrf) = f.login().await?;
    let base = [
        ("cookie", admin.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/customers",
            &[("cookie", &admin), ("content-type", "application/json")],
            r#"{"name":"张三"}"#
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    for query in [
        "page=1&page=2",
        "page=1&%70age=2",
        "unknown=1",
        "page_size=101",
        "sort=name%3BDROP%20TABLE",
        "search=%FF",
        "search=%ZZ",
    ] {
        assert_eq!(
            f.request(
                "GET",
                &format!("/api/customers?{query}"),
                &[("cookie", &admin)],
                Body::empty()
            )
            .await?
            .0,
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    for json in [r#"{"name":"a","name":"b"}"#, r#"{"name":{"x":1,"x":2}}"#] {
        assert_eq!(
            f.request("POST", "/api/customers", &base, json).await?.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut repeated = base.to_vec();
    repeated.push(("x-kenvoy-csrf", &csrf));
    assert_eq!(
        f.request("POST", "/api/customers", &repeated, r#"{"name":"张三"}"#)
            .await?
            .0,
        StatusCode::BAD_REQUEST
    );
    let duplicate = format!("{admin}; {admin}");
    assert_eq!(
        f.request(
            "GET",
            "/_kenvoy/session",
            &[("cookie", &duplicate)],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request("POST", "/api/customers", &base, vec![b'a'; BODY_LIMIT + 1])
            .await?
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut stale = base.to_vec();
    stale.push(("x-kenvoy-generation", "0"));
    assert_eq!(
        f.request("POST", "/api/customers", &stale, r#"{"name":"张三"}"#)
            .await?
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.request(
            "GET",
            "/api/_kenvoy_users",
            &[("cookie", &admin)],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("PUT", "/api/customers", &base, Body::empty())
            .await?
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        f.request("GET", "/unknown", &[], Body::empty()).await?.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("GET", "/customers", &[], Body::empty()).await?.0,
        StatusCode::NOT_FOUND
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn roles_logout_rotation_and_security_headers_are_enforced() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    f.auth.add_user("user", PASSWORD, Role::User).await?;
    let (user, _) = f.login_as("user").await?;
    assert_eq!(
        f.request("GET", "/api/customers", &[("cookie", &user)], Body::empty())
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let (cookie, csrf) = f.login().await?;
    let (status, headers, body) = f
        .request(
            "POST",
            "/_kenvoy/logout",
            &[
                ("cookie", &cookie),
                ("origin", "http://127.0.0.1:3000"),
                ("x-kenvoy-csrf", &csrf),
                ("content-type", "application/json"),
            ],
            "{}",
        )
        .await?;
    assert_eq!(status, StatusCode::OK);
    let next = headers
        .get(header::SET_COOKIE)
        .ok_or("缺少新会话")?
        .to_str()?;
    assert!(next.contains("HttpOnly"));
    assert!(next.contains("SameSite=Lax"));
    assert!(next.contains("Path=/"));
    assert!(!next.contains("Domain="));
    assert!(!next.starts_with(&cookie));
    assert!(serde_json::from_slice::<Value>(&body)?["user"].is_null());
    assert_eq!(
        headers.get("cache-control").ok_or("缺少缓存限制")?,
        "no-store"
    );
    assert_eq!(
        headers
            .get("x-content-type-options")
            .ok_or("缺少类型限制")?,
        "nosniff"
    );
    assert_eq!(
        f.request(
            "GET",
            "/api/customers",
            &[("cookie", &cookie)],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::UNAUTHORIZED
    );
    let request = Request::builder()
        .uri("/_kenvoy/session")
        .header("host", "evil.test")
        .body(Body::empty())?;
    assert_eq!(
        f.runtime.router().oneshot(request).await?.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request(
            "GET",
            "/_kenvoy/session",
            &[("sec-fetch-site", "cross-site")],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn new_sessions_public_writes_and_logins_have_bounded_rate_limits() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let (cookie, csrf) = f.context().await?;
    for _ in 1..20 {
        assert_eq!(
            f.request("GET", "/_kenvoy/session", &[], Body::empty())
                .await?
                .0,
            StatusCode::OK
        );
    }
    let (status, headers, _) = f
        .request("GET", "/_kenvoy/session", &[], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        headers.get(header::RETRY_AFTER).ok_or("缺少重试时间")?,
        "60"
    );
    for _ in 0..3 {
        let (status, headers, _) = f
            .request(
                "GET",
                "/_kenvoy/session",
                &[("cookie", &cookie)],
                Body::empty(),
            )
            .await?;
        assert_eq!(status, StatusCode::OK);
        assert!(headers.get(header::SET_COOKIE).is_none());
    }
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
        ("x-kenvoy-form", "/customers#form-1"),
    ];
    for _ in 0..20 {
        assert_eq!(
            f.request("POST", "/api/customers", &headers, r#"{"name":"受控提交"}"#)
                .await?
                .0,
            StatusCode::CREATED
        );
    }
    assert_eq!(
        f.request("POST", "/api/customers", &headers, r#"{"name":"限流"}"#)
            .await?
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        f.database
            .list("customers", Default::default())
            .await?
            .total,
        20
    );
    for _ in 0..5 {
        assert_eq!(
            f.request(
                "POST",
                "/_kenvoy/login",
                &headers,
                r#"{"username":"admin","password":"incorrect-password"}"#
            )
            .await?
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        f.request(
            "POST",
            "/_kenvoy/login",
            &headers,
            r#"{"username":"ADMIN","password":"incorrect-password"}"#
        )
        .await?
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert!(
        f.runtime
            .shared
            .request_limits
            .lock()
            .map_err(|_| "限流锁损坏")?
            .len()
            <= 4096
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn static_assets_require_real_files_and_admin_navigation_preserves_api_auth() -> TestResult {
    let mut f = Fixture::new(SOURCE).await?;
    let client = f.root.join("client");
    fs::create_dir_all(client.join("assets"))?;
    fs::write(
        client.join("index.html"),
        "<!doctype html><div id='root'></div>",
    )?;
    fs::write(
        client.join("assets/app.js"),
        "document.querySelector('#root').textContent='真实静态文件';",
    )?;
    fs::write(f.root.join("private.txt"), "host-secret")?;
    f.runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "http://127.0.0.1:3000".into(),
        Some(client),
    )
    .await?;
    let (status, headers, body) = f.request("GET", "/customers", &[], Body::empty()).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .ok_or("缺少类型")?
            .to_str()?
            .starts_with("text/html")
    );
    assert!(body.starts_with(b"<!doctype html>"));
    assert_eq!(
        f.request("GET", "/assets/app.js", &[], Body::empty())
            .await?
            .0,
        StatusCode::OK
    );
    let (status, headers, _) = f
        .request("GET", "/admin/customers", &[], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(header::LOCATION).ok_or("缺少跳转")?,
        "/admin/login?next=%2Fadmin%2Fcustomers"
    );
    assert_eq!(
        f.request(
            "GET",
            "/admin/login?next=%2Fadmin%2Fcustomers",
            &[],
            Body::empty()
        )
        .await?
        .0,
        StatusCode::OK
    );
    for path in [
        "/assets/../private.txt",
        "/assets/%2e%2e/private.txt",
        "/private.txt",
        "/app.sqlite",
        "/api/missing",
    ] {
        assert_eq!(
            f.request("GET", path, &[], Body::empty()).await?.0,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    assert_eq!(
        f.request("GET", "/api/customers", &[], Body::empty())
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn publication_is_cancel_safe_and_failed_migrations_keep_the_previous_generation()
-> TestResult {
    let f = Fixture::new(SOURCE).await?;
    f.database
        .create("customers", json!({"name":"保留记录"}), None)
        .await?;
    let mut changed = f.app.clone();
    changed.name = "第二代".into();
    let guard = f.runtime.shared.model.write().await;
    let runtime = f.runtime.clone();
    let task = tokio::spawn(async move { runtime.publish_prepared(changed, None, None).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !f
                .runtime
                .shared
                .publications
                .lock()
                .map_err(|_| "发布锁损坏")?
                .is_empty()
            {
                break Ok::<_, Box<dyn Error + Send + Sync>>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    task.abort();
    drop(guard);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.runtime.shared.model.read().await.generation == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(
        f.runtime.shared.model.read().await.application.name,
        "第二代"
    );
    assert_eq!(f.database.info().await?.generation, 2);
    let mut invalid = f.app.clone();
    invalid.resources[0].fields.remove(0);
    assert!(
        f.runtime
            .publish_prepared(invalid, None, None)
            .await
            .is_err()
    );
    assert_eq!(f.runtime.shared.model.read().await.generation, 2);
    assert_eq!(f.database.info().await?.generation, 2);
    assert_eq!(
        f.database
            .list("customers", Default::default())
            .await?
            .items[0]["name"],
        "保留记录"
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn real_loopback_http_serves_metadata_and_stops_gracefully() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        format!("http://{address}"),
        None,
    )
    .await?;
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        runtime
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
    });
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(
            format!(
                "GET /_kenvoy/application HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    let mut response = vec![];
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response)).await??;
    let response = String::from_utf8(response)?;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("客户应用"));
    assert!(!response.contains("app.tsx"));
    stop.send(()).map_err(|_| "服务已提前关闭")?;
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn database_failures_do_not_expose_sql_or_filesystem_paths() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let (cookie, _) = f.login().await?;
    // 只损坏此测试独占临时数据库，验证错误响应的真实失败路径。
    let connection = rusqlite::Connection::open(f.root.join("app.sqlite"))?;
    connection.execute_batch("DROP TABLE customers;")?;
    drop(connection);
    let (status, _, body) = f
        .request(
            "GET",
            "/api/customers",
            &[("cookie", &cookie)],
            Body::empty(),
        )
        .await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let text = String::from_utf8(body.to_vec())?;
    assert!(!text.contains("SELECT"));
    assert!(!text.contains("app.sqlite"));
    assert!(!text.contains("no such table"));
    assert!(!text.contains(&f.root.to_string_lossy().to_string()));
    assert_eq!(
        serde_json::from_str::<Value>(&text)?["error"]["code"],
        "KV5050"
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn https_cookies_and_strict_origin_ignore_untrusted_forwarded_headers() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    let runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "https://example.test".into(),
        None,
    )
    .await?;
    let request = Request::builder()
        .uri("/_kenvoy/session")
        .header("host", "example.test")
        .body(Body::empty())?;
    let response = runtime.router().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .ok_or("缺少 Cookie")?
        .to_str()?
        .to_owned();
    assert!(cookie.contains("; Secure"));
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), BODY_LIMIT).await?)?;
    let csrf = body["csrf_token"].as_str().ok_or("缺少 CSRF")?;
    let request = Request::builder()
        .method("POST")
        .uri("/api/customers")
        .header("host", "example.test")
        .header("cookie", cookie.split(';').next().ok_or("Cookie无效")?)
        .header("origin", "https://evil.test")
        .header("x-forwarded-host", "example.test")
        .header("forwarded", "host=example.test;proto=https")
        .header("x-kenvoy-csrf", csrf)
        .header("content-type", "application/json")
        .header("x-kenvoy-form", "/customers#form-1")
        .body(Body::from(r#"{"name":"伪造来源"}"#))?;
    assert_eq!(
        runtime.router().oneshot(request).await?.status(),
        StatusCode::FORBIDDEN
    );
    let request = Request::builder()
        .uri("/_kenvoy/session")
        .header("host", "evil.test")
        .header("x-forwarded-host", "example.test")
        .body(Body::empty())?;
    assert_eq!(
        runtime.router().oneshot(request).await?.status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        HttpRuntime::new(
            f.app.clone(),
            f.database.clone(),
            f.auth.clone(),
            "http://example.test".into(),
            None
        )
        .await
        .is_err()
    );
    f.database.shutdown().await?;
    Ok(())
}

const UPLOAD_SOURCE: &str = r#"export default <app name="附件测试"><resource name="documents"><text name="title" required unique/><file name="attachments" maxFiles={2} maxSize={4194304} accept={["application/pdf","image/*"]}/><file name="private_files"/></resource><page path="/submit"><form resource="documents" fields={["title","attachments"]}/></page></app>;"#;

#[tokio::test]
async fn uploaded_files_are_session_scoped_then_follow_record_permissions_and_atomic_updates()
-> TestResult {
    let f = Fixture::new(UPLOAD_SOURCE).await?;
    let (cookie, csrf) = f.context().await?;
    let (other, other_csrf) = f.context().await?;
    let upload_headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/submit#form-1"),
        ("x-kenvoy-filename", "%E8%B5%84%E6%96%99.pdf"),
        ("content-type", "application/pdf"),
    ];
    let pdf = b"%PDF-1.7\nisolated-upload-fixture\n%%EOF";
    let (status, _, body) = f
        .request(
            "POST",
            "/_kenvoy/uploads/documents/attachments",
            &upload_headers,
            pdf.to_vec(),
        )
        .await?;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let file = serde_json::from_slice::<Value>(&body)?["data"].clone();
    let file_path = format!(
        "/_kenvoy/files/{}",
        file["id"].as_str().ok_or("附件缺少id")?
    );
    let (status, headers, content) = f
        .request("GET", &file_path, &[("cookie", &cookie)], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&content[..], pdf);
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()?
            .starts_with("attachment;")
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(
        f.request("GET", &file_path, &[("cookie", &other)], Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    let write_headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/submit#form-1"),
        ("content-type", "application/json"),
    ];
    let other_headers = [
        ("cookie", other.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", other_csrf.as_str()),
        ("x-kenvoy-form", "/submit#form-1"),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/documents",
            &other_headers,
            json!({"title":"越权引用","attachments":[file.clone()]}).to_string()
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, _, body) = f
        .request(
            "POST",
            "/api/documents",
            &write_headers,
            json!({"title":"原记录","attachments":[file.clone()]}).to_string(),
        )
        .await?;
    let record = serde_json::from_slice::<Value>(&body)?;
    let record_path = format!(
        "/api/documents/{}",
        record["data"]["id"].as_i64().ok_or("缺少记录")?
    );
    assert_eq!(
        f.request("GET", &file_path, &[("cookie", &cookie)], Body::empty())
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request("DELETE", &file_path, &write_headers, Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    let (admin, admin_csrf) = f.login().await?;
    assert_eq!(
        f.request("GET", &file_path, &[("cookie", &admin)], Body::empty())
            .await?
            .2
            .as_ref(),
        pdf
    );
    let admin_headers = [
        ("cookie", admin.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", admin_csrf.as_str()),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/documents",
            &admin_headers,
            json!({"title":"挪用已绑定文件","attachments":[file.clone()]}).to_string()
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut forged = file.clone();
    forged["name"] = json!("伪造元数据.pdf");
    assert_eq!(
        f.request(
            "PATCH",
            &record_path,
            &admin_headers,
            json!({"attachments":[forged]}).to_string()
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        f.request("GET", &file_path, &[("cookie", &admin)], Body::empty())
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request(
            "PATCH",
            &record_path,
            &admin_headers,
            r#"{"attachments":[]}"#
        )
        .await?
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request("GET", &file_path, &[("cookie", &admin)], Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, _, body) = f
        .request(
            "POST",
            "/_kenvoy/uploads/documents/attachments",
            &upload_headers,
            pdf.to_vec(),
        )
        .await?;
    let pending = serde_json::from_slice::<Value>(&body)?["data"].clone();
    let pending_path = format!(
        "/_kenvoy/files/{}",
        pending["id"].as_str().ok_or("缺少暂存id")?
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/documents",
            &write_headers,
            json!({"title":"原记录","attachments":[pending.clone()]}).to_string()
        )
        .await?
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.request("GET", &pending_path, &[("cookie", &cookie)], Body::empty())
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/documents",
            &write_headers,
            json!({"title":"删除测试","attachments":[pending]}).to_string()
        )
        .await?
        .0,
        StatusCode::CREATED
    );
    let rows = f.database.list("documents", Default::default()).await?;
    let delete_id = rows
        .items
        .iter()
        .find(|row| row["title"] == "删除测试")
        .and_then(|row| row["id"].as_i64())
        .ok_or("缺少删除记录")?;
    assert_eq!(
        f.request(
            "DELETE",
            &format!("/api/documents/{delete_id}"),
            &admin_headers,
            Body::empty()
        )
        .await?
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        f.request("GET", &pending_path, &[("cookie", &admin)], Body::empty())
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    f.database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn upload_authorization_mime_and_body_limits_do_not_weaken_json_boundaries() -> TestResult {
    let f = Fixture::new(UPLOAD_SOURCE).await?;
    let path = "/_kenvoy/uploads/documents/attachments";
    assert_eq!(
        f.request(
            "POST",
            path,
            &[("origin", "http://127.0.0.1:3000")],
            b"%PDF-1.7".to_vec()
        )
        .await?
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (cookie, csrf) = f.context().await?;
    assert_eq!(
        f.request(
            "POST",
            path,
            &[("cookie", &cookie), ("origin", "http://127.0.0.1:3000")],
            b"%PDF-1.7".to_vec()
        )
        .await?
        .0,
        StatusCode::FORBIDDEN
    );
    let mut headers = vec![
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("x-kenvoy-form", "/submit#form-1"),
        ("x-kenvoy-filename", "example.pdf"),
        ("content-type", "application/pdf"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/_kenvoy/uploads/documents/private_files",
            &headers,
            b"%PDF-1.7".to_vec()
        )
        .await?
        .0,
        StatusCode::UNAUTHORIZED
    );
    headers[4].1 = "..%2Fprivate.pdf";
    assert_eq!(
        f.request("POST", path, &headers, b"%PDF-1.7".to_vec())
            .await?
            .0,
        StatusCode::BAD_REQUEST
    );
    headers[4].1 = "fake.png";
    headers[5].1 = "image/png";
    assert_eq!(
        f.request(
            "POST",
            path,
            &headers,
            b"<script>not-an-image</script>".to_vec()
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    headers[4].1 = "active.svg";
    headers[5].1 = "image/svg+xml";
    assert_eq!(
        f.request(
            "POST",
            path,
            &headers,
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec()
        )
        .await?
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    headers[4].1 = "large.pdf";
    headers[5].1 = "application/pdf";
    let mut large = vec![b' '; 2 * BODY_LIMIT + 1024];
    large[..8].copy_from_slice(b"%PDF-1.7");
    let (status, _, body) = f.request("POST", path, &headers, large).await?;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let file = serde_json::from_slice::<Value>(&body)?["data"].clone();
    let file_path = format!(
        "/_kenvoy/files/{}",
        file["id"].as_str().ok_or("缺少文件id")?
    );
    let (status, response_headers, body) = f
        .request("HEAD", &file_path, &[("cookie", &cookie)], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
    assert_eq!(
        response_headers[header::CONTENT_LENGTH]
            .to_str()?
            .parse::<usize>()?,
        2 * BODY_LIMIT + 1024
    );
    assert_eq!(
        f.request("POST", path, &headers, vec![b' '; 4 * BODY_LIMIT + 1])
            .await?
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    headers[5].1 = "application/json";
    assert_eq!(
        f.request(
            "POST",
            "/api/documents",
            &headers,
            vec![b' '; BODY_LIMIT + 1]
        )
        .await?
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    headers.push(("x-kenvoy-generation", "999999"));
    headers[5].1 = "application/pdf";
    assert_eq!(
        f.request("POST", path, &headers, b"%PDF-1.7".to_vec())
            .await?
            .0,
        StatusCode::CONFLICT
    );
    f.database.shutdown().await?;
    Ok(())
}
