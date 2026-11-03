use std::sync::Arc;

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    application::resources::OptionsQuery,
    domain::model::{Operation, Role},
};

use super::{
    ApiError, ClientPeer, Shared, check_generation, cookie, header_once, identity, projection,
    request::{decode_uri, id, limit_requests, list_query, no_query, parse_json, peer_key},
    require_write, role,
};

pub(super) async fn application(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    let mut projection = projection::application(
        &model.application,
        role(session.as_ref()),
        model.generation,
        &model.i18n,
    );
    if shared.development.read().await.is_some() {
        projection["development"] = json!(true);
    }
    Ok(Json(projection))
}

pub(super) async fn options(
    State(shared): State<Arc<Shared>>,
    Extension(peer): Extension<ClientPeer>,
    Path((resource, field_name)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    let query: OptionsQuery = serde_json::from_value(input)
        .map_err(|_| ApiError::bad("选项查询仅接受 dependencies/search/page/page_size/selected"))?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let field = shared.resources.options_field(
        &model.application,
        &resource,
        &field_name,
        role(Some(&session)),
    )?;
    let form = header_once(&headers, "x-kenvoy-form")?
        .map(decode_uri)
        .transpose()?;
    let options = field.authorize(form.as_deref())?;
    limit_requests(&shared, [(format!("options:{}", peer_key(peer.0)), 240)])?;
    let result = options.query(query).await?;
    Ok(Json(
        json!({"data":result.data,"selected":result.selected,"total":result.total,"page":result.page,"page_size":result.page_size,"generation":model.generation}),
    ))
}

pub(super) async fn list(
    State(shared): State<Arc<Shared>>,
    Path(resource): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let access = shared.resources.authorize(
        &model.application,
        &resource,
        Operation::List,
        role(session.as_ref()),
        None,
        None,
    )?;
    let query = list_query(&uri, &model.application, &resource)?;
    let result = access.list(query).await?;
    Ok(Json(
        json!({"data":result.items,"page":result.page,"page_size":result.page_size,"total":result.total,"generation":model.generation}),
    ))
}

pub(super) async fn read(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let access = shared.resources.authorize(
        &model.application,
        &resource,
        Operation::Read,
        role(session.as_ref()),
        None,
        None,
    )?;
    let record = access.get(id(&record)?).await?;
    Ok(Json(json!({"data":record})))
}

pub(super) async fn create(
    State(shared): State<Arc<Shared>>,
    Extension(peer): Extension<ClientPeer>,
    Path(resource): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let actual_role = role(Some(&session));
    let form = header_once(&headers, "x-kenvoy-form")?
        .map(decode_uri)
        .transpose()?;
    let access = shared.resources.authorize(
        &model.application,
        &resource,
        Operation::Create,
        actual_role,
        form.as_deref(),
        input.as_object(),
    )?;
    if actual_role != Role::Admin {
        let token = cookie(&headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
        let hash = Sha256::digest(token.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        limit_requests(
            &shared,
            [
                (format!("create:{}", peer_key(peer.0)), 60),
                (format!("create-session:{hash}"), 20),
            ],
        )?;
    }
    let create = access.prepare_create(input)?;
    let owner = super::uploads::owner(&headers)?;
    let record = create.execute(owner).await?;
    Ok((StatusCode::CREATED, Json(json!({"data":record}))).into_response())
}

pub(super) async fn update(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    no_query(&uri)?;
    let session = require_write(&shared, &headers).await?;
    let input = parse_json(&headers, &body)?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let access = shared.resources.authorize(
        &model.application,
        &resource,
        Operation::Update,
        role(Some(&session)),
        None,
        input.as_object(),
    )?;
    let update = access.prepare_update(input)?;
    let record = update
        .execute(id(&record)?, super::uploads::owner(&headers)?)
        .await?;
    Ok(Json(json!({"data":record})))
}

pub(super) async fn delete(
    State(shared): State<Arc<Shared>>,
    Path((resource, record)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    no_query(&uri)?;
    if !body.is_empty() {
        return Err(ApiError::bad("DELETE 不接受请求体"));
    }
    let session = require_write(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let access = shared.resources.authorize(
        &model.application,
        &resource,
        Operation::Delete,
        role(Some(&session)),
        None,
        None,
    )?;
    access.delete(id(&record)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
