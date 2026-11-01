use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use crate::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Auto,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    pub language: Language,
}

/// 入口由调用方明确给出；不读取任何框架专用配置。
#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    pub entry_points: Vec<std::path::PathBuf>,
    pub parse: ParseOptions,
    pub resolve: ResolveOptions,
}

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
        Ok(crate::parser::parse(path, source, options))
    }

    pub fn parse_file(
        &self,
        path: &Path,
        options: ParseOptions,
    ) -> Result<ParseOutput, EngineError> {
        let source = fs::read_to_string(path).map_err(|source| EngineError::Io {
            path: path.into(),
            source,
        })?;
        self.parse_source(path, &source, options)
    }

    pub fn resolve_module(
        &self,
        importer: &Path,
        specifier: &str,
        options: &ResolveOptions,
    ) -> Result<ResolvedModule, ResolveError> {
        crate::module::resolve(importer, specifier, options)
    }

    /// 遍历静态模块依赖并记录循环；本方法不会保留跨调用缓存。
    pub fn analyze_project(
        &self,
        root: &Path,
        options: AnalyzeOptions,
    ) -> Result<ProgramIR, EngineError> {
        analyze_with(root, options, |path, parse| self.parse_file(path, parse))
    }
}

/// 完整与增量分析共享遍历及诊断顺序，缓存仅替换文件解析这一个步骤。
pub(crate) fn analyze_with(
    root: &Path,
    mut options: AnalyzeOptions,
    mut parse: impl FnMut(&Path, ParseOptions) -> Result<ParseOutput, EngineError>,
) -> Result<ProgramIR, EngineError> {
    let root = root.canonicalize().map_err(|source| EngineError::Io {
        path: root.into(),
        source,
    })?;
    options.resolve.root = root.clone();
    let mut program = ProgramIR {
        format_version: GENERIC_IR_VERSION,
        modules: vec![],
        module_graph: ModuleGraph::default(),
        module_bindings: ModuleBindingsIR::default(),
        diagnostics: vec![],
    };
    if options.entry_points.is_empty() {
        program.diagnostics.push(Diagnostic {
            code: "KE0002".into(),
            category: DiagnosticCategory::Configuration,
            severity: Severity::Error,
            path: root,
            span: None,
            message: "未指定项目分析入口".into(),
            suggestion: "通过 AnalyzeOptions.entry_points 提供至少一个源码入口".into(),
            related: vec![],
        });
        return Ok(program);
    }
    let mut pending = vec![];
    for entry in &options.entry_points {
        let path = if entry.is_absolute() {
            entry.clone()
        } else {
            root.join(entry)
        };
        match path.canonicalize() {
            Ok(path) if path.starts_with(&root) => pending.push(path),
            Ok(path) => program.diagnostics.push(resolve_diagnostic(
                &path,
                None,
                &ResolveError::OutsideRoot {
                    path: path.clone(),
                    root: root.clone(),
                },
            )),
            Err(source) => program
                .diagnostics
                .push(io_diagnostic(&path, source.to_string())),
        }
    }
    let mut visited = BTreeSet::new();
    let mut modules = BTreeMap::new();
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let parsed = match parse(&path, options.parse) {
            Ok(parsed) => parsed,
            Err(error) => {
                program
                    .diagnostics
                    .push(io_diagnostic(&path, error.to_string()));
                continue;
            }
        };
        program.diagnostics.extend(parsed.diagnostics);
        let Some(module) = parsed.module else {
            continue;
        };
        let mut dependencies = vec![];
        for import in &module.imports {
            if let Some(source) = &import.source {
                let type_only = import.type_only
                    || (!import.specifiers.is_empty()
                        && import
                            .specifiers
                            .iter()
                            .all(|specifier| specifier.type_only));
                dependencies.push((
                    source,
                    if type_only {
                        DependencyKind::TypeImport
                    } else {
                        DependencyKind::Import
                    },
                ));
            }
        }
        for export in &module.exports {
            if let Some(source) = &export.source {
                let type_only = export.type_only
                    || (!export.specifiers.is_empty()
                        && export
                            .specifiers
                            .iter()
                            .all(|specifier| specifier.type_only));
                dependencies.push((
                    source,
                    if type_only {
                        DependencyKind::TypeReexport
                    } else {
                        DependencyKind::Reexport
                    },
                ));
            }
        }
        for (source, kind) in dependencies {
            let specifier = match source.to_string() {
                Ok(specifier) => specifier,
                Err(_) => {
                    program.diagnostics.push(Diagnostic {
                        code: "KE2003".into(),
                        category: DiagnosticCategory::Module,
                        severity: Severity::Error,
                        path: path.clone(),
                        span: Some(source.span.clone()),
                        message: "模块说明符含无法无损转换为文件路径的 UTF-16 code units".into(),
                        suggestion: "使用可表示为有效 Unicode 字符串的模块路径".into(),
                        related: vec![],
                    });
                    continue;
                }
            };
            let target = match crate::module::resolve(&path, &specifier, &options.resolve) {
                Ok(target) => {
                    if let ResolvedModule::File { path } = &target {
                        pending.push(path.clone());
                    }
                    Some(target)
                }
                Err(error) => {
                    program.diagnostics.push(resolve_diagnostic(
                        &path,
                        Some(source.span.clone()),
                        &error,
                    ));
                    None
                }
            };
            program.module_graph.edges.push(ModuleEdge {
                importer: path.clone(),
                specifier,
                span: source.span.clone(),
                kind,
                target,
            });
        }
        modules.insert(path, module);
    }
    program.module_graph.nodes = visited.into_iter().collect();
    program.module_graph.edges.sort_by(|left, right| {
        (&left.importer, left.span.start).cmp(&(&right.importer, right.span.start))
    });
    program.module_graph.update_cycles();
    program.modules = modules.into_values().collect();
    crate::linker::link_project(&mut program);
    Ok(program)
}

fn io_diagnostic(path: &Path, message: String) -> Diagnostic {
    Diagnostic {
        code: "KE3001".into(),
        category: DiagnosticCategory::Io,
        severity: Severity::Error,
        path: path.into(),
        span: None,
        message: format!("读取源文件失败：{message}"),
        suggestion: "检查文件是否存在、编码是否为 UTF-8，以及文件读取权限".into(),
        related: vec![],
    }
}

fn resolve_diagnostic(path: &Path, span: Option<SourceSpan>, error: &ResolveError) -> Diagnostic {
    let (code, suggestion) = match error {
        ResolveError::NotFound { .. } => ("KE2001", "检查相对路径、文件扩展名和目录 index 文件"),
        ResolveError::OutsideRoot { .. } => (
            "KE2002",
            "将依赖放入分析根目录；禁止通过 .. 或符号链接越过根目录",
        ),
        ResolveError::UnsupportedSpecifier { .. } => (
            "KE2003",
            "使用相对模块路径，或在 ResolveOptions.externals 中精确声明外部模块",
        ),
        ResolveError::Io { .. } => ("KE2004", "检查模块路径、文件权限和分析根目录"),
    };
    Diagnostic {
        code: code.into(),
        category: DiagnosticCategory::Module,
        severity: Severity::Error,
        path: path.into(),
        span,
        message: error.to_string(),
        suggestion: suggestion.into(),
        related: vec![],
    }
}
