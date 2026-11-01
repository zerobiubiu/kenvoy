mod analyzer;
pub mod client_slice;
pub mod static_eval;

use std::path::Path;

use kengine::{Diagnostic, ProgramIR};

use crate::domain::{model::ApplicationIR, source::DslDiagnostic};

pub use analyzer::{AnalyzeResult, analyze};

use client_slice::ClientCodeTransformer;

#[derive(Debug, thiserror::Error)]
pub enum CompilationError {
    #[error("源码分析失败，共 {} 项诊断", .0.len())]
    Language(Vec<Diagnostic>),
    #[error("应用声明分析失败，共 {} 项诊断", .0.len())]
    Dsl(Vec<DslDiagnostic>),
}

pub fn compile(
    program: ProgramIR,
    entry: &Path,
    name: &str,
    transformer: &impl ClientCodeTransformer,
) -> Result<(ApplicationIR, ProgramIR), CompilationError> {
    if program.has_errors() {
        return Err(CompilationError::Language(program.diagnostics));
    }
    let result: AnalyzeResult = analyze(&program, entry, name, transformer);
    if result.has_errors() {
        return Err(CompilationError::Dsl(result.diagnostics));
    }
    let application = result
        .application
        .ok_or(CompilationError::Dsl(result.diagnostics))?;
    Ok((application, program))
}
