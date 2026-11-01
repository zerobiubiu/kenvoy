use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(crate) enum SpecifierKind {
    External,
    Relative,
    Unsupported,
}

pub(crate) fn classify_specifier(specifier: &str, externals: &BTreeSet<String>) -> SpecifierKind {
    if externals.contains(specifier) {
        SpecifierKind::External
    } else if (!specifier.starts_with("./") && !specifier.starts_with("../"))
        || specifier.contains(['\\', '\0', ':'])
    {
        SpecifierKind::Unsupported
    } else {
        SpecifierKind::Relative
    }
}

pub(crate) fn candidates(base: &Path, map_js_to_ts: bool) -> Vec<PathBuf> {
    let mut candidates = vec![base.to_path_buf()];
    let extension = base.extension().and_then(|extension| extension.to_str());
    if map_js_to_ts && matches!(extension, Some("js" | "jsx" | "mjs" | "cjs")) {
        for extension in match extension {
            Some("mjs") => &["mts"][..],
            Some("cjs") => &["cts"][..],
            _ => &["ts", "tsx"][..],
        } {
            candidates.push(base.with_extension(extension));
        }
    }
    if !matches!(
        extension,
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    ) {
        for extension in ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
            let mut appended = base.as_os_str().to_os_string();
            appended.push(format!(".{extension}"));
            candidates.push(PathBuf::from(appended));
        }
    }
    for extension in ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
        candidates.push(base.join(format!("index.{extension}")));
    }
    candidates
}
