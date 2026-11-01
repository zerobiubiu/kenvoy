use super::*;

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
