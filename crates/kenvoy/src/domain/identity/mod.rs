//! 用户名与密码的纯输入规则。

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct IdentityInputError(pub(crate) String);

pub(crate) fn normalize_username(value: &str) -> Result<String, IdentityInputError> {
    if !(3..=64).contains(&value.len())
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.@-".contains(&b))
    {
        return Err(IdentityInputError("用户名须为 3 至 64 个 ASCII 字符，以字母或数字开头，仅允许字母、数字、下划线、点、@ 和连字符".into()));
    }
    Ok(value.to_ascii_lowercase())
}

pub(crate) fn validate_password(value: &str) -> Result<(), IdentityInputError> {
    if !(12..=128).contains(&value.len())
        || value.chars().any(char::is_control)
        || value.chars().all(char::is_whitespace)
    {
        return Err(IdentityInputError(
            "密码须为 12 至 128 个 UTF-8 字节，不能全为空白或含控制字符".into(),
        ));
    }
    Ok(())
}
