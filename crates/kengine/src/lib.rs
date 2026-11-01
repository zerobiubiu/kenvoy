//! 通用 JS、TS、JSX、TSX 解析与模块分析引擎。
//!
//! 本库不解释 JSX 标签的业务含义，不执行用户代码，也不公开 SWC 内部类型。
//! 提供源码、通用表达式、模块依赖、作用域/绑定、增量会话和诊断，不做完整类型检查。

mod diagnostics;
mod engine;
mod incremental;
mod ir;
mod linker;
mod module;
mod parser;
mod semantic;
mod source;

pub use diagnostics::*;
pub use engine::*;
pub use incremental::*;
pub use ir::*;
pub use linker::*;
pub use module::*;
pub use semantic::*;
pub use source::*;
