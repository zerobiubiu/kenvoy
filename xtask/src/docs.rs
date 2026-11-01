//! 文档编码、本地链接与唯一入口可达性校验。

use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn verify(root: &Path) -> Result<()> {
    let (files, links) = inspect(root)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "通过", "markdown_files": files, "local_links": links,
        }))?
    );
    Ok(())
}

fn inspect(root: &Path) -> Result<(usize, usize)> {
    let root = root.canonicalize()?;
    let docs = root.join("docs");
    let mut files = Vec::new();
    markdown_files(&docs, &mut files)?;
    files.extend([root.join("AGENTS.md"), root.join("README.md")]);
    files.sort();
    let mut graph = BTreeMap::new();
    let mut link_count = 0;
    for file in &files {
        let content = fs::read_to_string(file)
            .map_err(|error| format!("文档不能按 UTF-8 读取：{}：{error}", file.display()))?;
        validate_text(&content).map_err(|error| format!("{}：{error}", file.display()))?;
        let mut targets = Vec::new();
        for target in link_targets(&content) {
            if external(&target) {
                continue;
            }
            let path = decode_path(target.split('#').next().unwrap_or_default())?;
            if path.is_empty() {
                continue;
            }
            let resolved = file.parent().ok_or("文档父目录不存在")?.join(path);
            let canonical = resolved
                .canonicalize()
                .map_err(|error| format!("失效链接：{} -> {target}：{error}", file.display()))?;
            targets.push(canonical);
            link_count += 1;
        }
        graph.insert(file.canonicalize()?, targets);
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![docs.join("README.md").canonicalize()?];
    while let Some(file) = pending.pop() {
        if !reachable.insert(file.clone()) {
            continue;
        }
        if let Some(targets) = graph.get(&file) {
            pending.extend(targets.iter().cloned());
        }
    }
    for file in &files {
        if file.starts_with(&docs) && !reachable.contains(&file.canonicalize()?) {
            return Err(format!("未从 docs/README.md 索引：{}", file.display()).into());
        }
    }
    Ok((files.len(), link_count))
}

fn markdown_files(directory: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!("文档目录不允许符号链接：{}", entry.path().display()).into());
        }
        if kind.is_dir() {
            markdown_files(&entry.path(), result)?;
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "md")
        {
            result.push(entry.path());
        }
    }
    Ok(())
}

fn validate_text(content: &str) -> std::result::Result<(), &'static str> {
    if content.contains(['\0', '\u{fffd}']) {
        return Err("包含 NUL 或 Unicode 替换字符");
    }
    if content.lines().any(|line| {
        line.trim_end_matches('\r')
            .chars()
            .last()
            .is_some_and(char::is_whitespace)
    }) {
        return Err("包含行尾空白");
    }
    Ok(())
}

fn external(target: &str) -> bool {
    if target.starts_with('#') {
        return true;
    }
    target.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.as_bytes()[0].is_ascii_alphabetic()
            && scheme
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
    })
}

fn decode_path(path: &str) -> Result<String> {
    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .ok_or("本地链接的百分号编码不完整")?;
            let high = (hex[0] as char)
                .to_digit(16)
                .ok_or("本地链接的百分号编码无效")?;
            let low = (hex[1] as char)
                .to_digit(16)
                .ok_or("本地链接的百分号编码无效")?;
            result.push((high * 16 + low) as u8);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    Ok(String::from_utf8(result)?)
}

/// 覆盖项目使用的内联链接，也支持尖括号空格路径和路径内的配对括号。
fn link_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = content;
    while let Some(label) = rest.find('[') {
        rest = &rest[label + 1..];
        let Some(close) = rest.find("](") else {
            continue;
        };
        let destination = &rest[close + 2..];
        rest = destination;
        let destination = destination.trim_start();
        let (target, tail) = if let Some(bracketed) = destination.strip_prefix('<') {
            let Some(end) = bracketed.find('>') else {
                continue;
            };
            (&bracketed[..end], &bracketed[end + 1..])
        } else {
            let mut depth = 0usize;
            let mut end = destination.len();
            for (index, character) in destination.char_indices() {
                match character {
                    '(' => depth += 1,
                    ')' if depth > 0 => depth -= 1,
                    ')' => {
                        end = index;
                        break;
                    }
                    character if character.is_whitespace() && depth == 0 => {
                        end = index;
                        break;
                    }
                    _ => {}
                }
            }
            (&destination[..end], &destination[end..])
        };
        let tail = tail.trim_start();
        let tail = if let Some(title) = tail.strip_prefix('"') {
            title
                .find('"')
                .map(|end| title[end + 1..].trim_start())
                .unwrap_or_default()
        } else {
            tail
        };
        if !target.is_empty()
            && let Some(after) = tail.strip_prefix(')')
        {
            targets.push(target.to_owned());
            rest = after;
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_preserve_titles_unicode_spaces_and_balanced_parentheses() -> Result<()> {
        assert_eq!(
            link_targets(
                "[说明](guide.md \"标题\") [空格](<a b.md>) [结构](a(b).md) [网络](https://example.test)"
            ),
            ["guide.md", "a b.md", "a(b).md", "https://example.test"]
        );
        assert_eq!(decode_path("%E6%96%87%E6%A1%A3/a%20b.md")?, "文档/a b.md");
        assert!(decode_path("bad%2").is_err());
        assert!(decode_path("bad%GG").is_err());
        assert!(decode_path("bad%FF").is_err());
        assert!(external("https://example.test"));
        assert!(external("#section"));
        assert!(!external("../file.md"));
        assert!(link_targets("历史命令：`node tools/verify-docs.mjs`；当时 tools/probes/frontend 已构建。\n```text\nnode tools/tests/http-cli.mjs\n```").is_empty());
        Ok(())
    }

    #[test]
    fn text_validation_rejects_corruption_and_trailing_whitespace() {
        assert!(validate_text("正常\r\n内容\n").is_ok());
        for input in ["空格 \r\n", "制表\t\n", "替换\u{fffd}", "零\0"] {
            assert!(validate_text(input).is_err());
        }
    }

    #[test]
    fn docs_must_be_linked_and_targets_must_exist() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "kenvoy-xtask-docs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs"))?;
        let result = (|| -> Result<()> {
            fs::write(root.join("AGENTS.md"), "规则\n")?;
            fs::write(root.join("README.md"), "入口\n")?;
            fs::write(root.join("docs/README.md"), "[主题](topic.md)\n")?;
            fs::write(root.join("docs/topic.md"), "主题\n")?;
            assert_eq!(inspect(&root)?, (4, 1));
            fs::write(root.join("docs/topic.md"), "[损坏](missing.md)\n")?;
            assert!(inspect(&root).is_err());
            fs::write(root.join("docs/topic.md"), "主题\n")?;
            fs::write(root.join("docs/README.md"), "遗漏主题\n")?;
            assert!(inspect(&root).is_err());
            Ok(())
        })();
        fs::remove_dir_all(&root)?;
        result
    }
}
