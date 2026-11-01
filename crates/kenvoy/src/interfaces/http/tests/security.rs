use super::*;

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
