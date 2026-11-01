use std::path::Path;

use crate::{
    application::{
        analysis,
        error::{EngineError, ResolveError},
        options::{AnalyzeOptions, ParseOptions, ResolveOptions},
        ports::Host,
    },
    domain::{
        ir::{ParseOutput, ProgramIR},
        module::ResolvedModule,
    },
    infrastructure::NativeHost,
};

/// 无最近结果或全局业务状态的通用解析入口。
#[derive(Debug, Default)]
pub struct Engine;

impl Engine {
    pub fn new() -> Self {
        Self
    }

    /// 解析内存源码；语法错误返回结构诊断，不因恢复 AST 而被忽略。
    pub fn parse_source(
        &self,
        path: &Path,
        source: &str,
        options: ParseOptions,
    ) -> Result<ParseOutput, EngineError> {
        Ok(NativeHost.parse_source(path, source, options))
    }

    pub fn parse_file(
        &self,
        path: &Path,
        options: ParseOptions,
    ) -> Result<ParseOutput, EngineError> {
        analysis::parse_file(&NativeHost, path, options)
    }

    pub fn resolve_module(
        &self,
        importer: &Path,
        specifier: &str,
        options: &ResolveOptions,
    ) -> Result<ResolvedModule, ResolveError> {
        NativeHost.resolve_module(importer, specifier, options)
    }

    /// 遍历静态模块依赖并记录循环；本方法不会保留跨调用缓存。
    pub fn analyze_project(
        &self,
        root: &Path,
        options: AnalyzeOptions,
    ) -> Result<ProgramIR, EngineError> {
        analysis::analyze_project(&NativeHost, root, options)
    }
}
