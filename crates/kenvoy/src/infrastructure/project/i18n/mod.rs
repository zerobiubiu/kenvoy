//! 配置词典输入；默认文案提取属于领域，不要求先编译应用。
mod messages;

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::domain::i18n::{I18N_BUNDLE_VERSION, I18nBundle, validate_locales};

use super::{
    Project,
    error::{ProjectError as Error, Result, io},
    paths::{overlaps, safe_path},
};

/// 无显式配置即真实的旧应用单中文模式，不读取偶然存在的 locales 目录。
pub fn load(project: &Project) -> Result<I18nBundle> {
    let Some(config) = &project.config.i18n else {
        return Ok(I18nBundle::default());
    };
    project.check_compatibility()?;
    let directory = messages_dir(project)?.expect("显式 i18n 已配置词典目录");
    let mut messages = BTreeMap::new();
    for locale in &config.locales {
        let relative = config.messages_dir.join(format!("{locale}.json"));
        let path = safe_path(&project.root, &relative)?;
        if !path.starts_with(&directory) {
            return Err(Error::InvalidConfig(format!(
                "i18n 词典文件不能通过链接离开 messages_dir：{}",
                relative.display()
            )));
        }
        let catalog = match fs::read_to_string(&path) {
            Ok(source) => messages::parse(&source).map_err(|error| {
                Error::InvalidConfig(format!("i18n 词典 {} 无效：{error}", path.display()))
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if locale != &config.default_locale {
                    return Err(Error::InvalidConfig(format!(
                        "i18n 非默认语言 {locale} 缺少词典文件：{}",
                        path.display()
                    )));
                }
                BTreeMap::new()
            }
            Err(error) => return Err(io(&path, error)),
        };
        messages.insert(locale.clone(), catalog);
    }
    let bundle = I18nBundle {
        format_version: I18N_BUNDLE_VERSION,
        default_locale: config.default_locale.clone(),
        locales: config.locales.clone(),
        messages,
    };
    bundle
        .validate()
        .map_err(|error| Error::InvalidConfig(error.to_string()))?;
    Ok(bundle)
}

/// 允许 src/locales；数据、迁移及产物目录仍是不能成为词典输入的独立职责。
pub fn messages_dir(project: &Project) -> Result<Option<PathBuf>> {
    let Some(config) = &project.config.i18n else {
        return Ok(None);
    };
    validate_locales(&config.default_locale, &config.locales)
        .map_err(|error| Error::InvalidConfig(error.to_string()))?;
    let directory = safe_path(&project.root, &config.messages_dir)
        .map_err(|error| Error::InvalidConfig(format!("i18n.messages_dir 无效：{error}")))?;
    let relative = directory
        .strip_prefix(&project.root)
        .map_err(|_| Error::InvalidConfig("i18n.messages_dir 不能越过项目根目录".into()))?;
    if excluded_input(&config.messages_dir) || excluded_input(relative) {
        return Err(Error::InvalidConfig(
            "i18n.messages_dir 不能位于 target 或 .env* 等不参与构建指纹和开发监听的目录".into(),
        ));
    }
    let persistent = safe_path(&project.root, &project.config.database.path)?;
    let data = persistent
        .parent()
        .ok_or_else(|| Error::InvalidConfig("数据库必须有持久化目录".into()))?;
    let migrations = safe_path(&project.root, std::path::Path::new("migrations"))?;
    let current_data = project.database.parent().unwrap_or(&project.root);
    if directory == project.root
        || overlaps(&directory, data)
        || (current_data != data && overlaps(&directory, current_data))
        || overlaps(&directory, &project.out_dir)
        || overlaps(&directory, &migrations)
    {
        return Err(Error::InvalidConfig(
            "i18n.messages_dir 必须是项目内独立于数据库、正式 migrations 和构建输出的目录，且不能是项目根".into(),
        ));
    }
    match fs::metadata(&directory) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(Error::InvalidConfig(format!(
                "i18n.messages_dir 必须是目录：{}",
                directory.display()
            )));
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(io(&directory, error));
        }
        _ => {}
    }
    Ok(Some(directory))
}

fn excluded_input(path: &Path) -> bool {
    path.components().any(|component| {
        component.as_os_str().to_str().is_some_and(|name| {
            name.eq_ignore_ascii_case("target")
                || name
                    .get(..4)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(".env"))
        })
    })
}
