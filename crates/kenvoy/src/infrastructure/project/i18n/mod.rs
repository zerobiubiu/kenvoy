//! 根 translations.json 的只读装载与显式 Schema 准备；不读取源码或业务库。
mod files;
mod messages;
mod schema;

#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::domain::i18n::{I18N_BUNDLE_VERSION, I18nBundle, LanguageOption};

use super::{
    Project,
    error::{ProjectError as Error, Result},
};

pub use schema::schema;

#[derive(Debug)]
pub struct Preparation {
    pub bundle: I18nBundle,
    pub schema_updated: bool,
    pub translations_created: bool,
}

/// 只读验证根词典；无源码交付也不能绕过此输入契约。
pub fn load(project: &Project) -> Result<I18nBundle> {
    project.check_compatibility()?;
    let schema = schema(&project.config.languages)?;
    load_validated(project, &schema::validator(&schema)?)
}

/// 先验证配置，再原子刷新 Schema；已有词典无论有效与否都不会被改写。
pub fn prepare(project: &Project, create_missing: bool) -> Result<Preparation> {
    project.check_compatibility()?;
    let schema = schema(&project.config.languages)?;
    let validator = schema::validator(&schema)?;
    let schema_updated = files::refresh_schema(&project.root, &schema::encode(&schema)?)?;
    let translations_created = create_missing && files::create_translations(&project.root)?;
    Ok(Preparation {
        bundle: load_validated(project, &validator)?,
        schema_updated,
        translations_created,
    })
}

fn load_validated(project: &Project, validator: &jsonschema::Validator) -> Result<I18nBundle> {
    let bytes = files::read(&project.root, "translations.json")?.ok_or_else(|| {
        Error::InvalidConfig(
            "缺少根 translations.json；请先执行 kenvoy i18n，或手工创建包含 \"$schema\":\"./language.schema.json\" 的词典".into(),
        )
    })?;
    let document = messages::parse(&bytes)
        .map_err(|error| Error::InvalidConfig(format!("translations.json 无效：{error}")))?;
    validator.validate(&document).map_err(|error| {
        let keyword = error.schema_path.as_str().rsplit('/').next().unwrap_or("schema");
        let reason = match keyword {
            "const" => "$schema 必须为 ./language.schema.json",
            "required" | "minProperties" => "根对象必须包含 $schema，每条消息必须覆盖全部已配置语言",
            "oneOf" => "每种语言必须且只能提供代码或别名中的一个",
            "additionalProperties" => "存在未配置的语言代码或别名",
            "minLength" => "译文必须是非空字符串",
            "type" => "根对象和消息必须是对象，$schema 与译文必须是字符串",
            _ => "内容未满足语言 Schema 约束",
        };
        Error::InvalidConfig(format!(
            "translations.json 不符合 language.schema.json：位置 {}；规则 {keyword}（{}）：{reason}",
            error.instance_path, error.schema_path
        ))
    })?;
    normalize(project, document)
}

fn normalize(project: &Project, document: Value) -> Result<I18nBundle> {
    let config = &project.config.languages;
    let languages: Vec<_> = config
        .definitions
        .iter()
        .map(|(code, definition)| LanguageOption {
            code: code.clone(),
            alias: definition.alias.clone(),
            display: definition.display.clone(),
        })
        .collect();
    let identifiers: BTreeMap<&str, &str> = languages
        .iter()
        .flat_map(|language| {
            std::iter::once((language.code.as_str(), language.code.as_str())).chain(
                language
                    .alias
                    .as_deref()
                    .map(|alias| (alias, language.code.as_str())),
            )
        })
        .collect();
    let mut messages: BTreeMap<_, BTreeMap<String, String>> = config
        .definitions
        .keys()
        .map(|code| (code.clone(), BTreeMap::new()))
        .collect();
    let Value::Object(document) = document else {
        return Err(Error::InvalidConfig("translations.json 必须是对象".into()));
    };
    for (mut key, translations) in document {
        if key == "$schema" {
            continue;
        }
        let Value::Object(translations) = translations else {
            return Err(Error::InvalidConfig(format!(
                "translations.json 词条 {key:?} 必须是对象"
            )));
        };
        let mut translations = translations.into_iter().peekable();
        while let Some((identifier, text)) = translations.next() {
            let Value::String(text) = text else {
                return Err(Error::InvalidConfig(format!(
                    "translations.json 词条 {key:?} 的译文必须是字符串"
                )));
            };
            let catalog = identifiers
                .get(identifier.as_str())
                .and_then(|code| messages.get_mut(*code))
                .ok_or_else(|| Error::InvalidConfig(format!("未知翻译语言 {identifier:?}")))?;
            let message_key = if translations.peek().is_some() {
                key.clone()
            } else {
                std::mem::take(&mut key)
            };
            catalog.insert(message_key, text);
        }
    }
    drop(identifiers);
    let bundle = I18nBundle {
        format_version: I18N_BUNDLE_VERSION,
        fallback: config.fallback.clone(),
        languages,
        messages,
    };
    bundle
        .validate()
        .map_err(|error| Error::InvalidConfig(error.to_string()))?;
    Ok(bundle)
}
