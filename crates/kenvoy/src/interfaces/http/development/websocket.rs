use std::{sync::Arc, time::Duration};

use axum::{
    extract::{
        FromRequestParts, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::watch;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{self, client::IntoClientRequest, protocol::WebSocketConfig},
};

use super::super::{
    ClientPeer,
    request::{limit_requests, peer_key},
};
use super::{ApiError, SECRET_HEADER, Shared, current_proxy, header_once, path, unavailable};

pub(in crate::interfaces::http) async fn websocket(
    State(shared): State<Arc<Shared>>,
    request: Request,
) -> Result<Response, ApiError> {
    let proxy = current_proxy(&shared).await?;
    let peer = request
        .extensions()
        .get::<ClientPeer>()
        .copied()
        .unwrap_or(ClientPeer(None));
    let (mut parts, _body) = request.into_parts();
    let headers = &parts.headers;
    let uri = parts.uri.clone();
    if *proxy.stopped.borrow() {
        return Err(unavailable());
    }
    if header_once(headers, "origin")? != Some(shared.origin.as_str())
        || header_once(headers, "sec-websocket-protocol")? != Some("vite-hmr")
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "KV5013",
            "开发 WebSocket 必须使用同源 vite-hmr 协议",
        ));
    }
    limit_requests(&shared, [(format!("vite-ws:{}", peer_key(peer.0)), 60)])?;
    let ws = WebSocketUpgrade::from_request_parts(&mut parts, &())
        .await
        .map_err(|_| ApiError::bad("开发 WebSocket 握手无效"))?;
    let permit = proxy
        .sockets
        .clone()
        .try_acquire_owned()
        .map_err(|_| unavailable())?;
    let mut request = format!("ws://{}{}", proxy.endpoint, path(&uri)?)
        .into_client_request()
        .map_err(|_| ApiError::bad("WebSocket URL 无效"))?;
    request
        .headers_mut()
        .insert(SECRET_HEADER, proxy.secret.clone());
    request.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static("vite-hmr"),
    );
    let config = WebSocketConfig::default()
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024))
        .max_write_buffer_size(2 * 1024 * 1024);
    let (upstream, response) = tokio::time::timeout(
        Duration::from_secs(3),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    if response
        .headers()
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        != Some("vite-hmr")
    {
        return Err(unavailable());
    }
    let stopped = proxy.stopped.subscribe();
    Ok(ws
        .protocols(["vite-hmr"])
        .max_message_size(1024 * 1024)
        .max_frame_size(1024 * 1024)
        .max_write_buffer_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            bridge(socket, upstream, stopped).await;
        }))
}

async fn bridge(
    mut browser: WebSocket,
    mut upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut stopped: watch::Receiver<bool>,
) {
    loop {
        if *stopped.borrow() {
            break;
        }
        tokio::select! {
            _=stopped.changed()=>break,
            message=browser.next()=>{
                let Some(Ok(message))=message else{break;};
                let message=match message{Message::Text(text)=>tungstenite::Message::Text(text.as_str().into()),Message::Binary(bytes)=>tungstenite::Message::Binary(bytes),Message::Ping(bytes)=>tungstenite::Message::Ping(bytes),Message::Pong(bytes)=>tungstenite::Message::Pong(bytes),Message::Close(_)=>break};
                if !matches!(tokio::time::timeout(Duration::from_secs(2),upstream.send(message)).await,Ok(Ok(()))){break;}
            },
            message=upstream.next()=>{
                let Some(Ok(message))=message else{break;};
                let message=match message{tungstenite::Message::Text(text)=>Message::Text(text.as_str().into()),tungstenite::Message::Binary(bytes)=>Message::Binary(bytes),tungstenite::Message::Ping(bytes)=>Message::Ping(bytes),tungstenite::Message::Pong(bytes)=>Message::Pong(bytes),tungstenite::Message::Close(_)=>break,tungstenite::Message::Frame(_)=>continue};
                if !matches!(tokio::time::timeout(Duration::from_secs(2),browser.send(message)).await,Ok(Ok(()))){break;}
            },
        }
    }
    let _ = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(200),
            browser.send(Message::Close(None))
        ),
        tokio::time::timeout(
            Duration::from_millis(200),
            upstream.send(tungstenite::Message::Close(None))
        )
    );
}
