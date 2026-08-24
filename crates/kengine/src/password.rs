//! 密码强度校验（纯逻辑，无 I/O）。
//!
//! 默认不校验（`None`）；预设档位 `weak` / `medium` / `strong`；
//! 或自定义规则（`custom` 档位），如最短长度、大小写、数字、符号、不得包含用户名等。

#[allow(clippy::struct_excessive_bools)] // 密码策略本质是布尔规则集合，展开为字段最为直白
#[derive(Debug, Clone, Default)]
pub struct PasswordPolicy {
    /// 最短长度。
    pub min_length: Option<usize>,
    /// 要求大写字母。
    pub require_upper: bool,
    /// 要求小写字母。
    pub require_lower: bool,
    /// 要求数字。
    pub require_digit: bool,
    /// 要求符号（非字母数字）。
    pub require_symbol: bool,
    /// 不得包含用户名（忽略大小写）。
    pub reject_username: bool,
}

impl PasswordPolicy {
    /// 无校验策略。
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// 宽松：仅最短长度 6。
    #[must_use]
    pub fn weak() -> Self {
        Self {
            min_length: Some(6),
            ..Self::default()
        }
    }

    /// 中等：长度 8 + 小写与数字。
    #[must_use]
    pub fn medium() -> Self {
        Self {
            min_length: Some(8),
            require_lower: true,
            require_digit: true,
            ..Self::default()
        }
    }

    /// 强：长度 10 + 大小写、数字、符号。
    #[must_use]
    pub fn strong() -> Self {
        Self {
            min_length: Some(10),
            require_upper: true,
            require_lower: true,
            require_digit: true,
            require_symbol: true,
            ..Self::default()
        }
    }

    /// 校验密码，返回失败提示列表（空 = 通过）。
    #[must_use]
    pub fn validate(&self, password: &str, username: &str) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(min) = self.min_length
            && password.chars().count() < min
        {
            errors.push(format!("密码长度至少 {min} 位"));
        }
        if self.require_upper && !password.chars().any(|c| c.is_ascii_uppercase()) {
            errors.push("密码需包含大写字母".to_string());
        }
        if self.require_lower && !password.chars().any(|c| c.is_ascii_lowercase()) {
            errors.push("密码需包含小写字母".to_string());
        }
        if self.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
            errors.push("密码需包含数字".to_string());
        }
        if self.require_symbol && !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
            errors.push("密码需包含符号".to_string());
        }
        if self.reject_username
            && !username.is_empty()
            && password
                .to_ascii_lowercase()
                .contains(&username.to_ascii_lowercase())
        {
            errors.push("密码不能包含用户名".to_string());
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_always_passes() {
        let policy = PasswordPolicy::none();
        assert!(policy.validate("1", "u").is_empty());
        assert!(policy.validate("", "u").is_empty());
    }

    #[test]
    fn weak_enforces_min_length() {
        let policy = PasswordPolicy::weak();
        assert!(
            policy
                .validate("abc12", "u")
                .contains(&"密码长度至少 6 位".to_string())
        );
        assert!(policy.validate("abcdef", "u").is_empty());
    }

    #[test]
    fn strong_enforces_all_rules() {
        let policy = PasswordPolicy::strong();
        let ok = "Abc123!@";
        assert_eq!(ok.chars().count(), 8);
        assert_eq!(policy.validate(ok, "u"), vec!["密码长度至少 10 位"]);

        let ok = "Abc123!xyz";
        assert!(policy.validate(ok, "u").is_empty());
        assert!(
            policy
                .validate("abc123!xyz", "u")
                .contains(&"密码需包含大写字母".to_string())
        );
    }

    #[test]
    fn custom_rules_apply() {
        let policy = PasswordPolicy {
            min_length: Some(12),
            reject_username: true,
            ..PasswordPolicy::default()
        };
        assert!(
            policy
                .validate("alice_secret123", "alice")
                .contains(&"密码不能包含用户名".to_string())
        );
        assert!(
            policy
                .validate("secret123", "alice")
                .contains(&"密码长度至少 12 位".to_string())
        );
        assert!(policy.validate("very_long_ok", "alice").is_empty());
    }

    #[test]
    fn medium_rules() {
        let policy = PasswordPolicy::medium();
        assert!(policy.validate("password1", "u").is_empty());
        assert!(
            policy
                .validate("PASSWORD1", "u")
                .contains(&"密码需包含小写字母".to_string())
        );
        assert!(
            policy
                .validate("password", "u")
                .contains(&"密码需包含数字".to_string())
        );
    }
}
