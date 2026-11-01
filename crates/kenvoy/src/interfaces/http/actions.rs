use std::sync::Arc;

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{application::publication::AuthorizedAction, domain::model::Role};

use super::{
    ApiError, ClientPeer, Shared, check_generation, cookie,
    request::{limit_requests, no_query, parse_json, peer_key},
    require_write, role,
};

pub(super) async fn action(
    State(shared): State<Arc<Shared>>,
    Path(name): Path<String>,
    Extension(peer): Extension<ClientPeer>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let action: AuthorizedAction<'_, _> = model
        .action(&name, role(Some(&session)))
        .map_err(|error| ApiError::action(&name, error))?;
    let mut input = parse_json(&headers, &body)?;
    let object = input
        .as_object_mut()
        .ok_or_else(|| ApiError::bad("Action 请求必须为 JSON 对象"))?;
    if object.len() != 1 {
        return Err(ApiError::bad("Action 只接受 args 字段"));
    }
    let args = object
        .remove("args")
        .and_then(|value| match value {
            Value::Array(args) if args.len() <= 32 => Some(args),
            _ => None,
        })
        .ok_or_else(|| ApiError::bad("args 必须为不超过 32 项的 JSON 数组"))?;
    let token = cookie(&headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    let hash = Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    limit_requests(
        &shared,
        [
            (format!("action:{}", peer_key(peer.0)), 60),
            (format!("action-session:{hash}"), 20),
        ],
    )?;
    let value = action
        .call(args)
        .await
        .map_err(|error| ApiError::action(&name, error))?;
    Ok(Json(json!({"data":value,"generation":model.generation})))
}
