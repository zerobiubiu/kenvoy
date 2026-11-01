use std::path::{Component, Path, PathBuf};

use super::error::{ProjectError as Error, Result, io};

pub(super) fn overlaps(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        // 不存在的路径尾部尚不能 canonicalize，仍需遵循 Windows 文件名大小写规则。
        let left: Vec<_> = left
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        let right: Vec<_> = right
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        left.starts_with(&right) || right.starts_with(&left)
    }
    #[cfg(not(windows))]
    {
        left.starts_with(right) || right.starts_with(left)
    }
}

/// 对尚不存在的目标，解析最近的已存在祖先后再拼接，避免链接越过项目根。
pub(super) fn safe_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::InvalidConfig(format!(
            "路径必须是项目内相对路径且不含 ..：{}",
            relative.display()
        )));
    }
    for component in relative.components() {
        if let Component::Normal(segment) = component {
            let Some(segment) = segment.to_str() else {
                return Err(Error::InvalidConfig("项目路径必须为有效 Unicode".into()));
            };
            let stem = segment
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let reserved = matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || ["COM", "LPT"].iter().any(|prefix| {
                stem.strip_prefix(prefix).is_some_and(|number| {
                    (number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'))
                        || matches!(number, "¹" | "²" | "³")
                })
            });
            if segment
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                || segment.ends_with(['.', ' '])
                || reserved
                || [".git", ".hg", ".svn", "node_modules", ".kenvoy"]
                    .iter()
                    .any(|blocked| segment.eq_ignore_ascii_case(blocked))
            {
                return Err(Error::InvalidConfig(format!(
                    "项目路径包含保留目录、设备名或不可移植组件：{segment}"
                )));
            }
        }
    }
    let joined = root.join(relative);
    let mut ancestor = joined.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(
            ancestor
                .file_name()
                .ok_or_else(|| Error::InvalidConfig("无法解析项目路径".into()))?
                .to_owned(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| Error::InvalidConfig("无法解析项目路径祖先".into()))?;
    }
    let mut resolved = ancestor
        .canonicalize()
        .map_err(|error| io(ancestor, error))?;
    for segment in suffix.iter().rev() {
        resolved.push(segment);
    }
    if !resolved.starts_with(root) {
        return Err(Error::InvalidConfig(format!(
            "路径超出项目根：{}",
            relative.display()
        )));
    }
    Ok(resolved)
}
