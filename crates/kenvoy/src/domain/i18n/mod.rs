//! 原生语言配置与规范化词典的纯规则；不读取文件或业务数据。
mod config;
mod decode;
mod locale;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use config::LanguagesConfig;

pub const I18N_BUNDLE_VERSION: u32 = 2;
pub const DEFAULT_FALLBACK: &str = "zh-CN";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LanguageOption {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct I18nBundle {
    pub format_version: u32,
    pub fallback: String,
    pub languages: Vec<LanguageOption>,
    #[serde(deserialize_with = "decode::messages")]
    pub messages: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct I18nError(pub String);

impl Default for I18nBundle {
    fn default() -> Self {
        let config = LanguagesConfig::default();
        let mut messages = BTreeMap::new();
        let languages = config
            .definitions
            .into_iter()
            .map(|(code, definition)| {
                messages.insert(code.clone(), BTreeMap::new());
                LanguageOption {
                    code,
                    alias: definition.alias,
                    display: definition.display,
                }
            })
            .collect();
        Self {
            format_version: I18N_BUNDLE_VERSION,
            fallback: config.fallback,
            languages,
            messages,
        }
    }
}

impl I18nBundle {
    pub fn validate(&self) -> Result<(), I18nError> {
        if self.format_version != I18N_BUNDLE_VERSION {
            return Err(I18nError(format!(
                "不支持 i18n 词典格式 {}，当前支持 {I18N_BUNDLE_VERSION}",
                self.format_version
            )));
        }
        config::validate(
            &self.fallback,
            self.languages.iter().map(|language| {
                (
                    language.code.as_str(),
                    language.alias.as_deref(),
                    language.display.as_str(),
                )
            }),
        )?;
        for language in &self.languages {
            if !self.messages.contains_key(&language.code) {
                return Err(I18nError(format!(
                    "i18n 词典缺少已配置语言 {}",
                    language.code
                )));
            }
        }
        if self.messages.len() != self.languages.len() {
            return Err(I18nError("i18n 词典包含 languages 未声明的语言".into()));
        }
        let reference = &self.messages[&self.fallback];
        for (code, messages) in &self.messages {
            if messages.keys().ne(reference.keys()) {
                return Err(I18nError(format!(
                    "i18n 词典 {code} 的消息键必须完整覆盖所有配置语言"
                )));
            }
            if let Some((key, _)) = messages.iter().find(|(_, value)| value.is_empty()) {
                return Err(I18nError(format!(
                    "i18n 词典 {code} 的消息 {key:?} 不能为空字符串"
                )));
            }
        }
        Ok(())
    }
}
