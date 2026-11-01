use super::*;

#[tokio::test]
async fn publication_is_cancel_safe_and_failed_migrations_keep_the_previous_generation()
-> TestResult {
    let f = Fixture::new(SOURCE).await?;
    f.database
        .create("customers", json!({"name":"保留记录"}), None)
        .await?;
    let mut changed = f.app.clone();
    changed.name = "第二代".into();
    let guard = f.runtime.shared.model.write_for_test().await;
    let runtime = f.runtime.clone();
    let task = tokio::spawn(async move { runtime.publish_prepared(changed, None, None).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.runtime.shared.model.has_publications_for_test()? {
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
