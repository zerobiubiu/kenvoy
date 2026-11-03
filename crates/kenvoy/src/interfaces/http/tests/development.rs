use super::*;

#[tokio::test]
async fn first_navigation_waits_for_publication_without_holding_model_lock() -> TestResult {
    let f = Fixture::new(SOURCE).await?;
    f.runtime.enable_development(None).await?;
    f.runtime
        .set_development_status(DevelopmentPhase::Building, None, false)
        .await?;
    let timed_out = crate::interfaces::http::development::wait_ready(
        &f.runtime.shared,
        Duration::from_millis(20),
    )
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
        f.runtime
            .publish_prepared(next, None, None, Default::default()),
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
