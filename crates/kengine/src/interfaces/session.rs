use std::path::Path;

use crate::{
    application::{
        error::EngineError,
        incremental::{AnalysisState, AnalysisUpdate},
        options::AnalyzeOptions,
    },
    domain::ir::ProgramIR,
    infrastructure::NativeHost,
};

/// 调用方拥有的增量会话，不含全局状态，不执行代码或激活应用。
///
/// 每次读取可达文件的完整 UTF-8 内容，只有内容或解析选项改变时重新解析。
/// 依赖定位、模块图及跨模块分析每次重建，避免新增文件、重命名、externals
/// 或扩展名优先级变化被错误缓存。当前不缓存整个文件系统或 SWC AST。
/// 内存上界为当前可达解析结果、前次图及一份最后有效 ProgramIR；不保存历史队列。
#[derive(Debug, Default)]
pub struct AnalysisSession {
    state: AnalysisState,
}

impl AnalysisSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cached_modules(&self) -> usize {
        self.state.cached_modules()
    }

    /// 仅语法、模块及语义诊断无 Error 的候选才替换本快照。
    /// 本快照有效不代表宿主的业务模型、数据库或运行时已验证。
    pub fn last_successful(&self) -> Option<&ProgramIR> {
        self.state.last_successful()
    }

    /// 主动释放源码缓存、模块图及最后有效结果；之后分析等同冷启动。
    pub fn clear(&mut self) {
        self.state.clear();
    }

    pub fn analyze_project(
        &mut self,
        root: &Path,
        options: AnalyzeOptions,
    ) -> Result<AnalysisUpdate, EngineError> {
        self.state.analyze_project(&NativeHost, root, options)
    }
}
