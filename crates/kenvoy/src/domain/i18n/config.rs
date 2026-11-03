use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{DEFAULT_FALLBACK, I18nError, locale};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LanguageDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguagesConfig {
    #[serde(default = "default_fallback")]
    pub fallback: String,
    #[serde(flatten)]
    pub definitions: BTreeMap<String, LanguageDefinition>,
}

fn default_fallback() -> String {
    DEFAULT_FALLBACK.into()
}

impl Default for LanguagesConfig {
    fn default() -> Self {
        Self {
            fallback: default_fallback(),
            definitions: [
                ("zh-CN", "cn", "简体中文"),
                ("en-US", "en", "English"),
                ("ko-KR", "ko", "한국어"),
            ]
            .into_iter()
            .map(|(code, alias, display)| {
                (
                    code.into(),
                    LanguageDefinition {
                        alias: Some(alias.into()),
                        display: display.into(),
                    },
                )
            })
            .collect(),
        }
    }
}

impl LanguagesConfig {
    pub fn validate(&self) -> Result<(), I18nError> {
        validate(
            &self.fallback,
            self.definitions.iter().map(|(code, definition)| {
                (
                    code.as_str(),
                    definition.alias.as_deref(),
                    definition.display.as_str(),
                )
            }),
        )
    }
}

/// 配置和产物共用相同规则；保留声明拼写，不推定显示名或别名。
pub(super) fn validate<'a>(
    fallback: &str,
    languages: impl Iterator<Item = (&'a str, Option<&'a str>, &'a str)> + Clone,
) -> Result<(), I18nError> {
    let mut identifiers = BTreeSet::new();
    let mut contains_fallback = false;
    for (code, _, display) in languages.clone() {
        if !locale::valid_locale(code) {
            return Err(I18nError(format!(
                "languages 包含非法语言代码 {code:?}；请使用 zh-CN、en-US 等 BCP 47 语言标识"
            )));
        }
        if !identifiers.insert(code.to_lowercase()) {
            return Err(I18nError(format!(
                "languages 包含大小写不敏感的重复语言代码 {code:?}"
            )));
        }
        if display.trim().is_empty() {
            return Err(I18nError(format!(
                "languages.{code}.display 必填且不能为空"
            )));
        }
        contains_fallback |= code == fallback;
    }
    if identifiers.is_empty() {
        return Err(I18nError("languages 至少需要声明一种语言".into()));
    }
    if !contains_fallback {
        return Err(I18nError(format!(
            "languages.fallback {fallback:?} 必须与已声明语言代码完全一致，不能使用 alias"
        )));
    }
    for (code, alias, _) in languages {
        if let Some(alias) = alias {
            if alias.trim().is_empty() {
                return Err(I18nError(format!("languages.{code}.alias 给出时不能为空")));
            }
            if !identifiers.insert(alias.to_lowercase()) {
                return Err(I18nError(format!(
                    "languages.{code}.alias {alias:?} 与语言代码或其他 alias 冲突（不区分大小写）"
                )));
            }
        }
    }
    Ok(())
}
