use std::collections::BTreeSet;

use crate::domain::model::{FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE, UploadedFile};

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

pub fn validate_file_constraints(constraints: &FileConstraints) -> Result<(), &'static str> {
    if !(1..=MAX_UPLOAD_SIZE).contains(&constraints.max_size) {
        return Err("maxSize 必须为 1 到 10485760 的整数");
    }
    if !(1..=MAX_UPLOAD_FILES).contains(&constraints.max_files) {
        return Err("maxFiles 必须为 1 到 20 的整数");
    }
    let mut seen = BTreeSet::new();
    for accepted in &constraints.accept {
        if !(valid_mime_type(accepted) || accepted.strip_suffix("/*").is_some_and(mime_token)) {
            return Err("accept 只允许规范小写 MIME 或 type/*，不接受后缀和 MIME 参数");
        }
        if !seen.insert(accepted) {
            return Err("accept 不能包含重复 MIME");
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

pub(super) fn validate_file_metadata(
    id: &str,
    name: &str,
    content_type: &str,
    size: u64,
) -> Result<(), &'static str> {
    if !valid_upload_id(id) {
        return Err("文件 id 必须为 64 位小写十六进制");
    }
    if name.is_empty()
        || name.chars().count() > 255
        || matches!(name, "." | "..")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err("文件名必须为不超过 255 个 Unicode 标量的非空名称，不能包含路径或控制字符");
    }
    if !(1..=MAX_UPLOAD_SIZE).contains(&size) {
        return Err("文件大小必须为 1 到 10485760 字节");
    }
    if !valid_mime_type(content_type) {
        return Err("文件 content_type 必须为不含参数的规范小写 MIME");
    }
    Ok(())
}

pub fn validate_uploaded_file(file: &UploadedFile) -> Result<(), &'static str> {
    validate_file_metadata(&file.id, &file.name, &file.content_type, file.size)
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
