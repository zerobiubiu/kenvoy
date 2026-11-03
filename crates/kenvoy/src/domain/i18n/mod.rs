//! 原生词典及展示词条规则；不读配置、文件或业务数据。
mod decode;
mod defaults;
mod locale;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use defaults::defaults;

pub const I18N_BUNDLE_VERSION: u32 = 1;
pub const DEFAULT_LOCALE: &str = "zh-CN";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct I18nBundle {
    pub format_version: u32,
    pub default_locale: String,
    pub locales: Vec<String>,
    #[serde(deserialize_with = "decode::messages")]
    pub messages: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct I18nError(pub String);

impl Default for I18nBundle {
    fn default() -> Self {
        Self {
            format_version: I18N_BUNDLE_VERSION,
            default_locale: DEFAULT_LOCALE.into(),
            locales: vec![DEFAULT_LOCALE.into()],
            messages: BTreeMap::from([(DEFAULT_LOCALE.into(), BTreeMap::new())]),
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
        validate_locales(&self.default_locale, &self.locales)?;
        for locale in &self.locales {
            if !self.messages.contains_key(locale) {
                return Err(I18nError(format!("i18n 词典缺少已配置语言 {locale}")));
            }
        }
        for (locale, messages) in &self.messages {
            if !self.locales.contains(locale) {
                return Err(I18nError(format!(
                    "i18n 词典语言 {locale} 未在 locales 中声明"
                )));
            }
            if messages.keys().any(String::is_empty) {
                return Err(I18nError(format!("i18n 词典 {locale} 的消息键不能为空")));
            }
        }
        Ok(())
    }
}

/// 配置与构建产物共同使用同一纯规则，不推定或修改语言标识。
pub fn validate_locales(default_locale: &str, locales: &[String]) -> Result<(), I18nError> {
    if locales.is_empty() {
        return Err(I18nError("i18n.locales 至少需要一种语言".into()));
    }
    for (index, locale) in locales.iter().enumerate() {
        if !locale::valid_locale(locale) {
            return Err(I18nError(format!(
                "i18n.locales 包含非法语言 {locale:?}；请使用 zh-CN、en-US 等 BCP 47 语言标识"
            )));
        }
        if locales[..index]
            .iter()
            .any(|previous| previous.eq_ignore_ascii_case(locale))
        {
            return Err(I18nError(format!("i18n.locales 包含重复语言 {locale}")));
        }
    }
    if !locales.iter().any(|locale| locale == default_locale) {
        return Err(I18nError(format!(
            "i18n.default_locale {default_locale:?} 必须与 locales 中的一种语言完全一致"
        )));
    }
    Ok(())
}
