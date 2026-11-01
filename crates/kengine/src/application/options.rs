use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::domain::language::Language;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    pub language: Language,
}

/// 入口由调用方明确给出；不读取任何框架专用配置。
#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    pub entry_points: Vec<std::path::PathBuf>,
    pub parse: ParseOptions,
    pub resolve: ResolveOptions,
}

/// 第一版仅解析源码文件及显式 external，不实现 npm 条件导出或 tsconfig aliases。
#[derive(Debug, Clone)]
pub struct ResolveOptions {
    pub root: PathBuf,
    /// 精确匹配的外部模块说明符，不隐式开放包的所有子路径。
    pub externals: BTreeSet<String>,
    pub map_js_to_ts: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            externals: BTreeSet::new(),
            map_js_to_ts: true,
        }
    }
}

pub(crate) fn language(path: &Path, options: ParseOptions) -> Option<Language> {
    match options.language {
        Language::Auto => match path.extension().and_then(|extension| extension.to_str()) {
            Some("ts" | "mts" | "cts") => Some(Language::TypeScript),
            Some("tsx") => Some(Language::Tsx),
            Some("js" | "mjs" | "cjs") => Some(Language::JavaScript),
            Some("jsx") => Some(Language::Jsx),
            _ => None,
        },
        language => Some(language),
    }
}
