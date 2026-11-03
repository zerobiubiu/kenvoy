//! CLI 词条提取与原生界面共用同一份编译内置文案，不维护另一套翻译副本。
use std::{
    collections::{BTreeMap, btree_map::Entry},
    sync::LazyLock,
};

use super::{FrontendError, Result};

type Messages = BTreeMap<String, String>;
static ZH: LazyLock<std::result::Result<Messages, String>> = LazyLock::new(|| {
    combine(
        include_str!("../../../frontend/src/domain/i18n/ui.zh-CN.json"),
        include_str!("../../../frontend/src/domain/i18n/errors.zh-CN.json"),
    )
});
static EN: LazyLock<std::result::Result<Messages, String>> = LazyLock::new(|| {
    combine(
        include_str!("../../../frontend/src/domain/i18n/ui.en-US.json"),
        include_str!("../../../frontend/src/domain/i18n/errors.en-US.json"),
    )
});

pub(crate) fn messages(locale: &str) -> Result<Option<&'static Messages>> {
    let base = locale.split('-').next().unwrap_or(locale);
    let value = if base.eq_ignore_ascii_case("zh") {
        &*ZH
    } else if base.eq_ignore_ascii_case("en") {
        &*EN
    } else {
        return Ok(None);
    };
    value
        .as_ref()
        .map(Some)
        .map_err(|error| FrontendError::Boundary(format!("内置语言词典无效：{error}")))
}

fn combine(ui: &str, errors: &str) -> std::result::Result<Messages, String> {
    let mut messages: Messages = serde_json::from_str(ui).map_err(|error| error.to_string())?;
    let errors: Messages = serde_json::from_str(errors).map_err(|error| error.to_string())?;
    for (key, value) in errors {
        match messages.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(entry) => return Err(format!("重复的内置词条：{}", entry.key())),
        }
    }
    Ok(messages)
}
