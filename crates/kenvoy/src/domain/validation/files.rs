use std::collections::BTreeSet;

use crate::domain::model::{
    FieldDefinition, FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE, UploadedFile,
};
use serde_json::Value;

use super::{ValidationError, errors::field_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileValidationError {
    MaxSize,
    MaxFiles,
    AcceptFormat,
    AcceptDuplicate,
    Id,
    Name,
    Size,
    Mime,
}

impl FileValidationError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MaxSize => "maxSize 必须为 1 到 10485760 的整数",
            Self::MaxFiles => "maxFiles 必须为 1 到 20 的整数",
            Self::AcceptFormat => "accept 只允许规范小写 MIME 或 type/*，不接受后缀和 MIME 参数",
            Self::AcceptDuplicate => "accept 不能包含重复 MIME",
            Self::Id => "文件 id 必须为 64 位小写十六进制",
            Self::Name => {
                "文件名必须为不超过 255 个 Unicode 标量的非空名称，不能包含路径或控制字符"
            }
            Self::Size => "文件大小必须为 1 到 10485760 字节",
            Self::Mime => "文件 content_type 必须为不含参数的规范小写 MIME",
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            Self::MaxSize => "validation.files.constraints.max_size",
            Self::MaxFiles => "validation.files.constraints.max_files",
            Self::AcceptFormat => "validation.files.constraints.accept",
            Self::AcceptDuplicate => "validation.files.constraints.accept_duplicate",
            Self::Id => "validation.files.id",
            Self::Name => "validation.files.name",
            Self::Size => "validation.files.size",
            Self::Mime => "validation.files.mime",
        }
    }

    pub fn into_validation(self, field: impl Into<String>) -> ValidationError {
        let error = ValidationError::new(
            "KV4014",
            Some(field.into()),
            self.message(),
            self.message_key(),
        );
        match self {
            Self::MaxSize | Self::Size => error
                .with_param("min", 1)
                .with_param("max", MAX_UPLOAD_SIZE),
            Self::MaxFiles => error
                .with_param("min", 1)
                .with_param("max", MAX_UPLOAD_FILES),
            Self::Name => error.with_param("max", 255),
            Self::Id => error.with_param("length", 64),
            _ => error,
        }
    }
}

impl std::fmt::Display for FileValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for FileValidationError {}

fn mime_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$%&'+-.^_`|~".contains(&byte)
        })
}

pub fn valid_mime_type(value: &str) -> bool {
    value
        .split_once('/')
        .is_some_and(|(kind, subtype)| mime_token(kind) && mime_token(subtype))
}

pub fn mime_matches_accept(content_type: &str, accept: &[String]) -> bool {
    accept.is_empty()
        || accept.iter().any(|allowed| {
            allowed == content_type
                || allowed.strip_suffix("/*").is_some_and(|kind| {
                    content_type
                        .split_once('/')
                        .is_some_and(|(actual, _)| actual == kind)
                })
        })
}

pub fn validate_file_constraints(constraints: &FileConstraints) -> Result<(), FileValidationError> {
    if !(1..=MAX_UPLOAD_SIZE).contains(&constraints.max_size) {
        return Err(FileValidationError::MaxSize);
    }
    if !(1..=MAX_UPLOAD_FILES).contains(&constraints.max_files) {
        return Err(FileValidationError::MaxFiles);
    }
    let mut seen = BTreeSet::new();
    for accepted in &constraints.accept {
        if !(valid_mime_type(accepted) || accepted.strip_suffix("/*").is_some_and(mime_token)) {
            return Err(FileValidationError::AcceptFormat);
        }
        if !seen.insert(accepted) {
            return Err(FileValidationError::AcceptDuplicate);
        }
    }
    Ok(())
}

pub fn valid_upload_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_file_metadata(
    id: &str,
    name: &str,
    content_type: &str,
    size: u64,
) -> Result<(), FileValidationError> {
    if !valid_upload_id(id) {
        return Err(FileValidationError::Id);
    }
    if name.is_empty()
        || name.chars().count() > 255
        || matches!(name, "." | "..")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(FileValidationError::Name);
    }
    if !(1..=MAX_UPLOAD_SIZE).contains(&size) {
        return Err(FileValidationError::Size);
    }
    if !valid_mime_type(content_type) {
        return Err(FileValidationError::Mime);
    }
    Ok(())
}

pub fn validate_uploaded_file(file: &UploadedFile) -> Result<(), FileValidationError> {
    validate_file_metadata(&file.id, &file.name, &file.content_type, file.size)
}

pub(super) fn validate_file_field(
    field: &FieldDefinition,
    value: &Value,
) -> Result<Value, ValidationError> {
    let constraints = field.upload.as_ref().ok_or_else(|| {
        field_error(
            field,
            "KV4014",
            "file 字段缺少上传约束",
            "validation.files.constraints.missing",
        )
    })?;
    validate_file_constraints(constraints)
        .map_err(|error| error.into_validation(field.name.clone()))?;
    let files = value.as_array().ok_or_else(|| {
        field_error(
            field,
            "KV4002",
            "文件字段必须为附件元数据数组",
            "validation.type.files",
        )
    })?;
    if field.required && files.is_empty() {
        return Err(field_error(
            field,
            "KV4001",
            "必填附件至少需要一个文件",
            "validation.required.files",
        ));
    }
    if files.len() > constraints.max_files {
        return Err(field_error(
            field,
            "KV4014",
            "附件数量超过 maxFiles",
            "validation.files.max_files",
        )
        .with_param("max", constraints.max_files));
    }
    let mut ids = BTreeSet::new();
    for file in files {
        let object = file
            .as_object()
            .filter(|object| {
                object.len() == 4
                    && ["id", "name", "content_type", "size"]
                        .iter()
                        .all(|key| object.contains_key(*key))
            })
            .ok_or_else(|| {
                field_error(
                    field,
                    "KV4014",
                    "附件必须且只能包含 id/name/content_type/size",
                    "validation.files.metadata",
                )
            })?;
        let metadata = object["id"]
            .as_str()
            .zip(object["name"].as_str())
            .zip(object["content_type"].as_str())
            .zip(object["size"].as_u64())
            .ok_or_else(|| {
                field_error(
                    field,
                    "KV4014",
                    "附件 id/name/content_type 必须为字符串，size 必须为正整数",
                    "validation.files.metadata_types",
                )
            })?;
        let (((id, name), content_type), size) = metadata;
        validate_file_metadata(id, name, content_type, size)
            .map_err(|error| error.into_validation(field.name.clone()))?;
        if !ids.insert(id) {
            return Err(field_error(
                field,
                "KV4014",
                "附件数组不能重复引用同一个文件",
                "validation.files.duplicate",
            ));
        }
        if size > constraints.max_size {
            return Err(field_error(
                field,
                "KV4014",
                "附件大小超过 maxSize",
                "validation.files.max_size",
            )
            .with_param("max", constraints.max_size));
        }
        if !mime_matches_accept(content_type, &constraints.accept) {
            return Err(field_error(
                field,
                "KV4014",
                "附件 MIME 不满足 accept",
                "validation.files.accept",
            ));
        }
    }
    Ok(value.clone())
}

/// 图片声明和客户端投影只接收明确的 HTTP(S) URL 或站内绝对路径。
pub fn valid_image_src(src: &str) -> bool {
    if src.is_empty()
        || src.trim() != src
        || src.contains('\\')
        || src.chars().any(char::is_control)
    {
        return false;
    }
    if src.starts_with('/') {
        return !src.starts_with("//");
    }
    let Some((scheme, remainder)) = src.split_once("://") else {
        return false;
    };
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || remainder
            .split(['/', '?', '#'])
            .next()
            .is_none_or(|authority| authority.is_empty() || authority.contains('@'))
    {
        return false;
    }
    let Ok(url) = url::Url::parse(src) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}
