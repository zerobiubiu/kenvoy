//! 原生词条提取和缺失报告只消费声明/交付词典，不连接数据库、不覆盖应用文件。
use std::collections::{BTreeMap, BTreeSet};

use kengine::AnalysisSession;
use serde_json::{Value, json};

use super::{
    compile,
    error::{Error, Result},
};
use crate::{
    domain::{
        i18n::{self as domain_i18n, I18nBundle},
        model::ApplicationIR,
    },
    infrastructure::{
        artifacts, frontend,
        project::{self, Project},
    },
    interfaces::cli::{self, I18nCommand},
};

pub(super) fn execute(
    project: &Project,
    session: &mut AnalysisSession,
    action: I18nCommand,
) -> Result<()> {
    let (application, packaged) = if project.entry.is_file() {
        (compile(project, session)?.0, None)
    } else {
        let output = artifacts::load(project)?;
        (output.application, Some(output.i18n))
    };
    match action {
        I18nCommand::Extract { locale } => {
            let defaults = extract(
                project,
                &application,
                packaged.as_deref(),
                locale.as_deref(),
            )?;
            cli::print_json(&defaults)?;
        }
        I18nCommand::Check => {
            let report = match packaged {
                Some(bundle) => check(&application, &bundle)?,
                None => check(&application, &project::i18n::load(project)?)?,
            };
            cli::print_json(&report)?;
        }
    }
    Ok(())
}

fn extract(
    project: &Project,
    application: &ApplicationIR,
    packaged: Option<&I18nBundle>,
    requested: Option<&str>,
) -> Result<BTreeMap<String, String>> {
    let configured = project.config.i18n.as_ref();
    let default = packaged
        .map(|bundle| bundle.default_locale.as_str())
        .or_else(|| configured.map(|config| config.default_locale.as_str()))
        .unwrap_or("zh-CN");
    let locale = requested.unwrap_or(default);
    let allowed = match (packaged, configured) {
        (Some(bundle), _) => bundle.locales.iter().any(|supported| supported == locale),
        (None, Some(config)) => config.locales.iter().any(|supported| supported == locale),
        (None, None) => locale == "zh-CN",
    };
    if !allowed {
        return Err(Error::InvalidConfig(format!(
            "提取语言 {locale} 不在配置的 i18n.locales 中；请显式声明支持语言"
        )));
    }
    let mut messages = builtin(locale, default)?.clone();
    // 当前声明的默认文案是提取源；现有翻译不会反向覆盖源文案，也不会被此命令写入。
    messages.extend(domain_i18n::defaults(application));
    Ok(messages)
}

fn check(application: &ApplicationIR, bundle: &I18nBundle) -> Result<Value> {
    let builtin_defaults = builtin(&bundle.default_locale, &bundle.default_locale)?;
    let source = domain_i18n::defaults(application);
    let keys: BTreeSet<_> = builtin_defaults.keys().chain(source.keys()).collect();
    let mut missing = BTreeMap::new();
    for locale in &bundle.locales {
        let builtins = frontend::messages(locale)?;
        let translated = bundle.messages.get(locale);
        let absent: Vec<_> = keys
            .iter()
            .filter_map(|key| {
                let key = key.as_str();
                let known = builtins.is_some_and(|catalog| catalog.contains_key(key))
                    || translated.is_some_and(|catalog| catalog.contains_key(key));
                (!known).then_some(key)
            })
            .collect();
        missing.insert(locale.as_str(), absent);
    }
    Ok(
        json!({"status":"passed","default_locale":bundle.default_locale,"locales":bundle.locales,"missing":missing}),
    )
}

fn builtin(locale: &str, fallback: &str) -> Result<&'static BTreeMap<String, String>> {
    if let Some(messages) = frontend::messages(locale)? {
        return Ok(messages);
    }
    if let Some(messages) = frontend::messages(fallback)? {
        return Ok(messages);
    }
    frontend::messages("zh-CN")?.ok_or_else(|| Error::InvalidConfig("内置默认语言词典缺失".into()))
}
