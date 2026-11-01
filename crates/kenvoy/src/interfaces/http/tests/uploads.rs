use super::*;

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
