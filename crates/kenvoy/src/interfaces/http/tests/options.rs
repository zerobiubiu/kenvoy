use super::*;

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
    f.runtime
        .publish_prepared(restricted, None, None, Default::default())
        .await?;
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
