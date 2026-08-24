//! HTML 转义（防 XSS）。

/// 转义文本中的 HTML 特殊字符。
#[must_use]
pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_special_chars() {
        assert_eq!(
            escape_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn leaves_plain_text_unchanged() {
        assert_eq!(escape_html("中文 abc 123 {{x}}"), "中文 abc 123 {{x}}");
    }
}
