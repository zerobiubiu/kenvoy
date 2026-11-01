use super::*;

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
