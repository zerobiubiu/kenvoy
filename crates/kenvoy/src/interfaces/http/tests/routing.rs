use super::*;

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
    for path in ["/", "/customers"] {
        let (status, headers, body) = f.request("GET", path, &[], Body::empty()).await?;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            headers
                .get(header::CONTENT_TYPE)
                .ok_or("缺少类型")?
                .to_str()?
                .starts_with("text/html")
        );
        assert!(body.starts_with(b"<!doctype html>"));
    }
    let (status, _, body) = f.request("HEAD", "/", &[], Body::empty()).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
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
        "/api",
        "/_kenvoy",
        "/_kenvoy/missing",
        "/not-a-page",
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
async fn declared_root_permissions_take_precedence_over_runtime_entry() -> TestResult {
    let source = SOURCE.replace("/private", "/");
    let mut f = Fixture::new(&source).await?;
    let client = f.root.join("client");
    fs::create_dir(&client)?;
    fs::write(
        client.join("index.html"),
        "<!doctype html><div id='root'></div>",
    )?;
    f.runtime = HttpRuntime::new(
        f.app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "http://127.0.0.1:3000".into(),
        Some(client),
    )
    .await?;
    assert_eq!(
        f.request("GET", "/", &[], Body::empty()).await?.0,
        StatusCode::UNAUTHORIZED
    );
    f.auth.add_user("visitor", PASSWORD, Role::User).await?;
    let (visitor, _) = f.login_as("visitor").await?;
    assert_eq!(
        f.request("GET", "/", &[("cookie", &visitor)], Body::empty())
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let (admin, _) = f.login().await?;
    let (status, headers, body) = f
        .request("GET", "/", &[("cookie", &admin)], Body::empty())
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .ok_or("缺少类型")?
            .to_str()?
            .starts_with("text/html")
    );
    assert!(body.starts_with(b"<!doctype html>"));
    f.database.shutdown().await?;
    Ok(())
}
