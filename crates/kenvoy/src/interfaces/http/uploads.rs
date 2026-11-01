//! 附件通过同源会话与资源字段策略访问，不进入静态资产目录。

use std::{fmt::Write, net::SocketAddr, sync::Arc};

use axum::{
    Extension, Json,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::{
    application::resources::{NewUpload, UploadField, upload_owner},
    domain::{
        model::{Operation, Role},
        validation::valid_upload_id,
    },
};

use super::{
    ApiError, Shared, check_generation, cookie, header_once,
    middleware::identity_with_token,
    request::{decode_uri, limit_requests, no_query, peer_key},
    require_write, role,
};

#[derive(Clone)]
pub(super) struct UploadRequest {
    resource: String,
    field: String,
    name: String,
    content_type: String,
    owner: String,
    expires_at: i64,
    generation: u64,
    pub max_size: usize,
}

pub(super) fn owner(headers: &HeaderMap) -> Result<String, ApiError> {
    let token = cookie(headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    Ok(upload_owner(&token))
}

/// 在读取大请求体之前完成授权；接收结束后仍核对代际和会话期限。
pub(super) async fn prepare(
    shared: &Shared,
    headers: &HeaderMap,
    uri: &Uri,
    peer: Option<SocketAddr>,
) -> Result<UploadRequest, ApiError> {
    no_query(uri)?;
    let tail = uri
        .path()
        .strip_prefix("/_kenvoy/uploads/")
        .ok_or_else(ApiError::missing)?;
    let (resource, field_name) = tail
        .split_once('/')
        .filter(|(_, field)| !field.contains('/'))
        .ok_or_else(ApiError::missing)?;
    let resource = decode_uri(resource)?;
    let field_name = decode_uri(field_name)?;
    let session = require_write(shared, headers).await?;
    let model = shared.model.read().await;
    check_generation(headers, model.generation)?;
    let field: UploadField<'_> =
        shared
            .resources
            .upload_field(&model.application, &resource, &field_name)?;
    let mode = header_once(headers, "x-kenvoy-upload-mode")?.unwrap_or("create");
    let operation = match mode {
        "create" => Operation::Create,
        "update" => Operation::Update,
        _ => return Err(ApiError::bad("上传模式只能为 create 或 update")),
    };
    let form = header_once(headers, "x-kenvoy-form")?
        .map(decode_uri)
        .transpose()?;
    field.authorize(operation, role(Some(&session)), form.as_deref())?;
    let name = header_once(headers, "x-kenvoy-filename")?
        .ok_or_else(|| ApiError::bad("缺少上传文件名"))?;
    if name.len() > 4096 {
        return Err(ApiError::bad("上传文件名过长"));
    }
    let name = decode_uri(name)?;
    if name.is_empty()
        || name.chars().count() > 255
        || matches!(name.as_str(), "." | "..")
        || name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(ApiError::bad("文件名不能包含路径、控制字符或超过255个字符"));
    }
    let content_type = header_once(headers, "content-type")?.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "KV5015",
            "上传必须声明文件 MIME 类型",
        )
    })?;
    if content_type.len() > 128 {
        return Err(ApiError::bad("文件 MIME 类型无效"));
    }
    let owner = owner(headers)?;
    limit_requests(
        shared,
        [
            (format!("upload:{}", peer_key(peer)), 120),
            (format!("upload-session:{owner}"), 60),
        ],
    )?;
    let max_size = field
        .max_size()
        .ok_or_else(|| ApiError::bad("上传大小限制无效"))?;
    Ok(UploadRequest {
        resource,
        field: field_name,
        name,
        content_type: content_type.into(),
        owner,
        expires_at: session.expires_at,
        generation: model.generation,
        max_size,
    })
}

pub(super) async fn upload(
    State(shared): State<Arc<Shared>>,
    Extension(upload): Extension<UploadRequest>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let model = shared.model.read().await;
    shared
        .resources
        .accept_upload(upload.expires_at, upload.generation, model.generation)?;
    let file = shared
        .resources
        .stage_upload(NewUpload {
            resource: upload.resource,
            field: upload.field,
            name: upload.name,
            content_type: upload.content_type,
            owner: upload.owner,
            expires_at: upload.expires_at,
            data: body.to_vec(),
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"data":file,"generation":model.generation})),
    )
        .into_response())
}

pub(super) async fn read(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    method: Method,
) -> Result<Response, ApiError> {
    if !valid_upload_id(&id) {
        return Err(ApiError::missing());
    }
    let download = match uri.query() {
        None | Some("") => false,
        Some("download=1") => true,
        _ => return Err(ApiError::bad("文件接口只接受 download=1")),
    };
    let (session, token) = identity_with_token(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let stored = shared
        .resources
        .file(
            &model.application,
            &id,
            role(session.as_ref()),
            session.is_some(),
            token.as_deref(),
        )
        .await?;
    let data = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(shared.resources.file_content(&id).await?)
    };
    let inline = !download
        && matches!(
            stored.file.content_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        );
    let mut disposition = String::with_capacity(80 + stored.file.name.len() * 3);
    disposition.push_str(if inline { "inline" } else { "attachment" });
    disposition.push_str("; filename=\"download\"; filename*=UTF-8''");
    for byte in stored.file.name.as_bytes() {
        write!(&mut disposition, "%{byte:02X}").map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "KV5050",
                "文件响应无法生成",
            )
        })?;
    }
    let mut response = data.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&stored.file.content_type).map_err(|_| {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "KV5050", "文件类型无效")
        })?,
    );
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, HeaderValue::from(stored.file.size));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "KV5050",
                "文件响应无法生成",
            )
        })?,
    );
    Ok(response)
}

pub(super) async fn remove(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    no_query(&uri)?;
    if !valid_upload_id(&id) {
        return Err(ApiError::missing());
    }
    if !body.is_empty() {
        return Err(ApiError::bad("删除暂存文件不接受请求体"));
    }
    require_write(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    shared
        .resources
        .delete_upload(&id, &owner(&headers)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
