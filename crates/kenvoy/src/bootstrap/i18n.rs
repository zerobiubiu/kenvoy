//! 原生语言配置与集中译文准备；不编译应用、不访问数据库、不覆盖已有译文。
use serde_json::json;

use super::error::Result;
use crate::{
    infrastructure::project::{self, Project},
    interfaces::cli,
};

pub(super) fn execute(project: &Project) -> Result<()> {
    let prepared = project::i18n::prepare(project, true)?;
    let message_count = prepared
        .bundle
        .messages
        .values()
        .next()
        .map_or(0, |messages| messages.len());
    cli::print_json(&json!({
        "status": "passed",
        "schema": "language.schema.json",
        "schema_updated": prepared.schema_updated,
        "translations": "translations.json",
        "translations_created": prepared.translations_created,
        "fallback": prepared.bundle.fallback,
        "languages": prepared.bundle.languages,
        "message_count": message_count,
    }))?;
    Ok(())
}
