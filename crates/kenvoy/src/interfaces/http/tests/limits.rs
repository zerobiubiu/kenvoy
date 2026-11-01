use super::*;

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
