use std::{
    collections::BTreeSet,
    net::SocketAddr,
    time::{Duration, Instant},
};

use axum::http::{HeaderMap, StatusCode, Uri};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    application::resources::{FieldFilter, FilterOperator, ListQuery},
    domain::{model::ApplicationIR, validation},
};

use super::{ApiError, Shared, header_once};

pub(super) fn no_query(uri: &Uri) -> Result<(), ApiError> {
    if uri.query().is_some_and(|query| !query.is_empty()) {
        Err(ApiError::bad("此接口不接受查询参数"))
    } else {
        Ok(())
    }
}
pub(super) fn parse_json(headers: &HeaderMap, body: &[u8]) -> Result<Value, ApiError> {
    let content_type = header_once(headers, "content-type")?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "KV5015",
            "请求必须使用 application/json",
        )
    })?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "KV5015",
            "请求必须使用 application/json",
        ));
    }
    validation::parse_record_json(body)
        .map_err(|_| ApiError::bad("请求必须为有效且不含重复字段的 JSON 对象"))
}

pub(super) fn peer_key(peer: Option<SocketAddr>) -> String {
    peer.map(|peer| peer.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}
pub(super) fn limit_requests<const N: usize>(
    shared: &Shared,
    keys: [(String, u32); N],
) -> Result<(), ApiError> {
    let now = Instant::now();
    let mut limits = shared.request_limits.lock().map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "KV5153",
            "请求限制状态暂不可用",
        )
    })?;
    limits.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
    if keys
        .iter()
        .any(|(key, max)| limits.get(key).is_some_and(|(_, count)| count >= max))
        || limits.len()
            + keys
                .iter()
                .filter(|(key, _)| !limits.contains_key(key))
                .count()
            > 4096
    {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "KV5129",
            "请求过于频繁，请稍后重试",
        ));
    }
    for (key, _) in keys {
        limits.entry(key).or_insert((now, 0)).1 += 1;
    }
    Ok(())
}
pub(super) fn limit_login(
    shared: &Shared,
    peer: Option<SocketAddr>,
    username: &str,
) -> Result<(), ApiError> {
    let hash = Sha256::digest(username.to_ascii_lowercase().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    limit_requests(
        shared,
        [
            (format!("login:{}", peer_key(peer)), 5),
            (format!("user:{hash}"), 5),
        ],
    )
}

pub(super) fn id(value: &str) -> Result<i64, ApiError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::bad("记录 id 必须为正整数"));
    }
    let id = value
        .parse::<i64>()
        .map_err(|_| ApiError::bad("记录 id 超出允许范围"))?;
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(ApiError::bad("记录 id 超出允许范围"));
    }
    Ok(id)
}

pub(super) fn list_query(
    uri: &Uri,
    app: &ApplicationIR,
    resource: &str,
) -> Result<ListQuery, ApiError> {
    let definition = app.resource(resource).ok_or_else(ApiError::missing)?;
    let raw = uri.query().unwrap_or("");
    if raw.len() > 8192 {
        return Err(ApiError::bad("查询参数过长"));
    }
    decode_uri(raw)?;
    // form_urlencoded 保留键值对顺序；先查重，不能先收集为 Map 后覆盖。
    let mut seen = BTreeSet::new();
    let mut query = ListQuery::default();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(key.to_string()) {
            return Err(ApiError::bad("查询参数不能重复"));
        }
        match key.as_ref() {
            "page" => {
                query.page = value
                    .parse()
                    .map_err(|_| ApiError::bad("page 必须为正整数"))?
            }
            "page_size" => {
                query.page_size = value
                    .parse()
                    .map_err(|_| ApiError::bad("page_size 必须为正整数"))?
            }
            "search" => query.search = Some(value.into_owned()),
            "sort" => query.sort = Some(value.into_owned()),
            "direction" => {
                query.descending = match value.as_ref() {
                    "asc" => false,
                    "desc" => true,
                    _ => return Err(ApiError::bad("direction 只能为 asc 或 desc")),
                }
            }
            key if key.starts_with("filter_") => {
                let field = &key[7..];
                if definition.field(field).is_none() {
                    return Err(ApiError::bad("筛选字段未在资源中声明"));
                }
                let value: Value = serde_json::from_str(&value).map_err(|_| {
                    ApiError::bad("筛选值必须为 JSON 标量，例如 25、true、null 或带引号的字符串")
                })?;
                if value.is_array() || value.is_object() {
                    return Err(ApiError::bad("筛选值必须为 JSON 标量"));
                }
                query.filters.push(FieldFilter {
                    field: field.into(),
                    operator: FilterOperator::Eq,
                    value,
                });
            }
            _ => return Err(ApiError::bad("存在不支持的查询参数")),
        }
    }
    Ok(query)
}

/// URL 解码前验证百分号及 UTF-8，禁止有损替换掩盖不同的参数或路由。
pub(super) fn decode_uri(raw: &str) -> Result<String, ApiError> {
    let mut decoded = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'%' {
            let code = bytes
                .get(offset + 1..offset + 3)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            let high = (code[0] as char)
                .to_digit(16)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            let low = (code[1] as char)
                .to_digit(16)
                .ok_or_else(|| ApiError::bad("URL 百分号编码无效"))?;
            decoded.push((high * 16 + low) as u8);
            offset += 3;
        } else {
            decoded.push(bytes[offset]);
            offset += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ApiError::bad("URL 必须为有效 UTF-8"))
}
