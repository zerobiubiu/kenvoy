//! 附件通过同源会话与资源字段策略访问，不进入静态资产目录。

use std::{
    fmt::Write,
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Extension, Json,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    application::{FieldKind, Operation, Role},
    database::NewUpload,
    validation::valid_upload_id,
};

use super::{
    ApiError, Shared, check_generation, cookie,
    handlers::{decode_uri, limit_requests, no_query, peer_key, policy},
    header_once, identity, require_write, role,
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

fn now() -> Result<i64, ApiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_secs()).ok())
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "KV5050", "系统时间无效"))
}

pub(super) fn owner(headers: &HeaderMap) -> Result<String, ApiError> {
    let token = cookie(headers)?.ok_or_else(|| ApiError::denied(Role::Anonymous))?;
    let digest = Sha256::digest(token.as_bytes());
    let mut hash = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        hash.push(HEX[(byte >> 4) as usize] as char);
        hash.push(HEX[(byte & 15) as usize] as char);
    }
    Ok(hash)
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
    let field = model
        .application
        .resource(&resource)
        .and_then(|resource| resource.field(&field_name))
        .filter(|field| field.kind == FieldKind::File)
        .ok_or_else(ApiError::missing)?;
    let constraints = field.upload.as_ref().ok_or_else(ApiError::missing)?;
    let mode = header_once(headers, "x-kenvoy-upload-mode")?.unwrap_or("create");
    let operation = match mode {
        "create" => Operation::Create,
        "update" => Operation::Update,
        _ => return Err(ApiError::bad("上传模式只能为 create 或 update")),
    };
    let form = header_once(headers, "x-kenvoy-form")?
        .map(decode_uri)
        .transpose()?;
    let fields = serde_json::Map::from_iter([(field_name.clone(), Value::Null)]);
    policy(
        &model.application,
        &resource,
        operation,
        role(Some(&session)),
        form.as_deref(),
        Some(&fields),
    )?;
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
    Ok(UploadRequest {
        resource,
        field: field_name,
        name,
        content_type: content_type.into(),
        owner,
        expires_at: session.expires_at,
        generation: model.generation,
        max_size: usize::try_from(
            constraints
                .max_size
                .min(crate::application::MAX_UPLOAD_SIZE),
        )
        .map_err(|_| ApiError::bad("上传大小限制无效"))?,
    })
}

pub(super) async fn upload(
    State(shared): State<Arc<Shared>>,
    Extension(upload): Extension<UploadRequest>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let model = shared.model.read().await;
    if upload.generation != model.generation {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "KV5010",
            "上传期间应用已更新，请重新选择文件",
        ));
    }
    if upload.expires_at <= now()? {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "KV5001",
            "会话已过期，请重新选择文件",
        ));
    }
    let file = shared
        .database
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
    let session = identity(&shared, &headers).await?;
    let model = shared.model.read().await;
    check_generation(&headers, model.generation)?;
    let stored = shared
        .database
        .file_info(&id)
        .await?
        .ok_or_else(ApiError::missing)?;
    model
        .application
        .resource(&stored.resource)
        .and_then(|resource| resource.field(&stored.field))
        .filter(|field| field.kind == FieldKind::File)
        .ok_or_else(ApiError::missing)?;
    if stored.record_id.is_some() {
        let fields = serde_json::Map::from_iter([(stored.field.clone(), Value::Null)]);
        policy(
            &model.application,
            &stored.resource,
            Operation::Read,
            role(session.as_ref()),
            None,
            Some(&fields),
        )?;
    } else if session.is_none() || stored.expires_at <= now()? || owner(&headers)? != stored.owner {
        return Err(ApiError::missing());
    }
    let data = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(
            shared
                .database
                .file_content(&id)
                .await?
                .ok_or_else(ApiError::missing)?,
        )
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
    if !shared
        .database
        .delete_upload(&id, &owner(&headers)?)
        .await?
    {
        return Err(ApiError::missing());
    }
    Ok(StatusCode::NO_CONTENT)
}
