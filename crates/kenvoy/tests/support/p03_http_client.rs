//! P03 的有界原始 HTTP 客户端。

use super::ProbeResult;
use std::{io, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(super) struct HttpResponse {
    pub(super) status: u16,
    pub(super) headers: String,
    pub(super) body: String,
}

pub(super) async fn request(
    address: SocketAddr,
    method: &str,
    target: &str,
    body: &str,
    authorization: Option<&str>,
) -> ProbeResult<HttpResponse> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(address).await?;
        let authorization = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        let head = format!(
            "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{authorization}\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body.as_bytes()).await?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await?;
        let response = String::from_utf8(bytes)?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .ok_or_else(|| io::Error::other("HTTP 响应缺少头部边界"))?;
        let status = headers
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| io::Error::other("HTTP 响应缺少状态码"))?
            .parse()?;
        Ok(HttpResponse {
            status,
            headers: headers.to_owned(),
            body: body.to_owned(),
        })
    })
    .await?
}
