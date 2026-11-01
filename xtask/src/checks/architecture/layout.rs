//! 只扫描自有源码；依赖、缓存及构建产物不进入行数或分层统计。

use super::{rust, typescript};
use crate::Result;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) const MAX_LINES: usize = 500;

pub(super) struct Source {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Default)]
pub(super) struct Stats {
    pub files: usize,
    pub rust: usize,
    pub typescript: usize,
    pub maximum_lines: usize,
}

pub(super) fn inspect(root: &Path) -> Result<Stats> {
    let mut errors = Vec::new();
    root_layout(
        root,
        "crates/kenvoy/src",
        &["main.rs"],
        &[
            "domain",
            "application",
            "infrastructure",
            "interfaces",
            "bootstrap",
        ],
        true,
        &mut errors,
    )?;
    root_layout(
        root,
        "crates/kengine/src",
        &["lib.rs"],
        &["domain", "application", "infrastructure", "interfaces"],
        true,
        &mut errors,
    )?;
    root_layout(
        root,
        "crates/kenvoy/frontend/src",
        &["main.tsx", "sdk.ts", "env.d.ts"],
        &[
            "app",
            "domain",
            "application",
            "infrastructure",
            "presentation",
        ],
        false,
        &mut errors,
    )?;
    let mut sources = Vec::new();
    collect(root, root, &mut sources)?;
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    let mut stats = Stats {
        files: sources.len(),
        ..Stats::default()
    };
    for source in &sources {
        let lines = source.text.lines().count();
        stats.maximum_lines = stats.maximum_lines.max(lines);
        if lines > MAX_LINES {
            errors.push(format!(
                "源码超过 {MAX_LINES} 行（实际 {lines} 行）：{}",
                source.path.display()
            ));
        }
    }
    stats.rust = rust::inspect(&sources, &mut errors)?;
    stats.typescript = typescript::inspect(&sources, &mut errors)?;
    if !errors.is_empty() {
        errors.sort();
        errors.dedup();
        return Err(format!("源码结构门禁失败：\n{}", errors.join("\n")).into());
    }
    Ok(stats)
}

fn root_layout(
    root: &Path,
    directory: &str,
    files: &[&str],
    layers: &[&str],
    rust_modules: bool,
    errors: &mut Vec<String>,
) -> Result<()> {
    let path = root.join(directory);
    let expected: BTreeSet<_> = files.iter().chain(layers).copied().collect();
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "源码目录名不是 UTF-8")?;
        actual.insert(name.clone());
        let kind = entry.file_type()?;
        let correct_kind = if files.contains(&name.as_str()) {
            kind.is_file()
        } else if layers.contains(&name.as_str()) {
            kind.is_dir()
        } else {
            false
        };
        if !correct_kind || kind.is_symlink() {
            errors.push(format!("源码顶层存在未约定条目：{directory}/{name}"));
        }
    }
    for missing in expected.difference(&actual.iter().map(String::as_str).collect()) {
        errors.push(format!("缺少真实分层入口：{directory}/{missing}"));
    }
    for layer in layers {
        let path = path.join(layer);
        if path.is_dir() && rust_modules && !path.join("mod.rs").is_file() {
            errors.push(format!("Rust 层缺少 mod.rs：{directory}/{layer}"));
        }
        if path.is_dir() && fs::read_dir(&path)?.next().is_none() {
            errors.push(format!("不允许空分层目录：{directory}/{layer}"));
        }
    }
    Ok(())
}

fn collect(root: &Path, directory: &Path, sources: &mut Vec<Source>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let kind = entry.file_type()?;
        if [
            ".git",
            "node_modules",
            "target",
            "dist",
            ".kenvoy",
            ".idea",
            ".vscode",
        ]
        .contains(&name.as_ref())
            || (directory == root && ["artifacts", ".impeccable"].contains(&name.as_ref()))
        {
            continue;
        }
        if kind.is_symlink() {
            return Err(format!("自有源码不允许符号链接：{}", entry.path().display()).into());
        }
        if kind.is_dir() {
            collect(root, &entry.path(), sources)?;
        } else if code_extension(&entry.path()) {
            sources.push(Source {
                path: entry.path().strip_prefix(root)?.to_path_buf(),
                text: fs::read_to_string(entry.path())?,
            });
        }
    }
    Ok(())
}

fn code_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs" | "css")
    )
}
