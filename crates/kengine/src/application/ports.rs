use std::{
    io,
    path::{Path, PathBuf},
};

use crate::domain::{ir::ParseOutput, module::ResolvedModule};

use super::{
    error::{EngineError, ResolveError},
    options::{ParseOptions, ResolveOptions},
};

/// 分析流程实际使用的宿主能力；实现由公开入口静态装配。
pub(crate) trait Host {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_source(&self, path: &Path) -> Result<String, EngineError>;
    fn parse_source(&self, path: &Path, source: &str, options: ParseOptions) -> ParseOutput;
    fn resolve_module(
        &self,
        importer: &Path,
        specifier: &str,
        options: &ResolveOptions,
    ) -> Result<ResolvedModule, ResolveError>;
}
