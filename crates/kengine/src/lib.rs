//! kengine — Kenvoy 核心引擎。
//!
//! 纯逻辑（无 I/O）：
//! - `ir` / `parse` — DSL XML → IR 中间表示与 golden 测试
//! - `expr` / `validation` / `workflow` / `password` — 表达式引擎 / 表单校验 / 审批流状态机 / 密码策略
//! - `html`（feature `html`，默认开）— IR → HTML 渲染器（SSR / 边缘渲染），内嵌 Tabler CSS + 图标 + 运行时
//!
//! 解析部分（`ir`/`parse`/`expr`/`validation`/`workflow`/`password`）在最小依赖下即可用；
//! 启用 `html` 带来渲染能力。无运行时副作用，便于未来 WASM 目标。

pub mod expr;
pub mod ir;
pub mod password;
pub mod validation;
pub mod workflow;

mod parse;

#[cfg(feature = "html")]
pub mod html;

pub use parse::{ParseError, SourceFile, build_project};

/// 引擎版本，与 workspace 版本一致。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 返回问候语。
///
/// # Examples
/// ```
/// assert_eq!(kengine::hello("kenvoy"), "hello, kenvoy");
/// ```
#[must_use]
pub fn hello(name: &str) -> String {
    format!("hello, {name}")
}

/// 引擎信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    /// 引擎名称
    pub name: &'static str,
    /// 引擎版本
    pub version: &'static str,
}

impl EngineInfo {
    /// 创建默认引擎信息。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: "kengine",
            version: VERSION,
        }
    }
}

impl Default for EngineInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for EngineInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

/// 初始化引擎，返回 [`EngineInfo`]。
///
/// 占位逻辑：校验环境并返回信息，后续可扩展为真实初始化。
#[must_use]
pub const fn init() -> EngineInfo {
    EngineInfo::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_works() {
        assert_eq!(hello("world"), "hello, world");
    }

    #[test]
    fn info_display() {
        let info = EngineInfo::new();
        assert_eq!(info.to_string(), format!("kengine {VERSION}"));
    }

    #[test]
    fn init_returns_info() {
        let info = init();
        assert_eq!(info.name, "kengine");
    }
}
