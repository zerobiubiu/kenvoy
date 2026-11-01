use serde::{Deserialize, Serialize};

/// 从一开始的行号与 Unicode 标量列号；列号不是字节数或 UTF-16 code units。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// 源文件内从零开始的半开字节范围，同时保留起止行列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub start_position: Position,
    pub end_position: Position,
}

impl SourceSpan {
    /// 从对应模块的原始源码恢复精确片段；无效边界返回 None。
    pub fn raw<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }
}

pub(crate) struct SourceIndex<'a> {
    source: &'a str,
    lines: Vec<usize>,
}

impl<'a> SourceIndex<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        let mut lines = vec![0];
        let mut chars = source.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            match ch {
                '\r' if chars.peek().is_some_and(|(_, next)| *next == '\n') => {}
                '\r' | '\n' | '\u{2028}' | '\u{2029}' => lines.push(offset + ch.len_utf8()),
                _ => {}
            }
        }
        Self { source, lines }
    }

    fn position(&self, offset: usize) -> Position {
        let line_index = self
            .lines
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let start = self.lines[line_index];
        Position {
            line: line_index + 1,
            column: self.source[start..offset].chars().count() + 1,
        }
    }

    pub(crate) fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan {
            start,
            end,
            start_position: self.position(start),
            end_position: self.position(end),
        }
    }
}
