use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::domain::{
    ir::{ParseOutput, ProgramIR},
    module::{ModuleGraph, affected_modules},
};

use super::{
    analysis::analyze_with,
    error::EngineError,
    options::{AnalyzeOptions, ParseOptions},
    ports::Host,
};

/// 本次实际解析与复用的文件；文件集合按规范路径排序。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalysisStats {
    pub parsed: Vec<PathBuf>,
    pub reused: Vec<PathBuf>,
    /// 前次可达、本次不可达或读取失败的缓存条目。
    pub evicted: Vec<PathBuf>,
    /// 源码或依赖解析发生变化的模块，以及其旧图/新图的传递导入方。
    /// 这不是“全部重解析”的列表：未变源码可复用语法和模块内语义。
    pub affected: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisUpdate {
    /// 当前文件状态的候选结果，可能含错误；调用方须检查 has_errors。
    pub program: ProgramIR,
    pub stats: AnalysisStats,
}

#[derive(Debug)]
struct CachedParse {
    options: ParseOptions,
    source: String,
    output: ParseOutput,
}

#[derive(Debug, Default)]
pub(crate) struct AnalysisState {
    cache: BTreeMap<PathBuf, CachedParse>,
    previous_graph: ModuleGraph,
    last_successful: Option<ProgramIR>,
}

impl AnalysisState {
    pub(crate) fn cached_modules(&self) -> usize {
        self.cache.len()
    }

    /// 仅语法、模块及语义诊断无 Error 的候选才替换本快照。
    /// 本快照有效不代表宿主的业务模型、数据库或运行时已验证。
    pub(crate) fn last_successful(&self) -> Option<&ProgramIR> {
        self.last_successful.as_ref()
    }

    /// 主动释放源码缓存、模块图及最后有效结果；之后分析等同冷启动。
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.previous_graph = ModuleGraph::default();
        self.last_successful = None;
    }

    pub(crate) fn analyze_project(
        &mut self,
        host: &impl Host,
        root: &Path,
        options: AnalyzeOptions,
    ) -> Result<AnalysisUpdate, EngineError> {
        let previous_paths: BTreeSet<_> = self.cache.keys().cloned().collect();
        let mut observed = BTreeSet::new();
        let mut stats = AnalysisStats::default();
        let program = analyze_with(host, root, options, |path, parse_options| {
            let source = host.read_source(path)?;
            observed.insert(path.to_path_buf());
            if let Some(cached) = self.cache.get(path)
                && cached.options == parse_options
                && cached.source == source
            {
                stats.reused.push(path.to_path_buf());
                return Ok(cached.output.clone());
            }
            let output = host.parse_source(path, &source, parse_options);
            stats.parsed.push(path.to_path_buf());
            self.cache.insert(
                path.to_path_buf(),
                CachedParse {
                    options: parse_options,
                    source,
                    output: output.clone(),
                },
            );
            Ok(output)
        })?;
        self.cache.retain(|path, _| observed.contains(path));
        stats.evicted = previous_paths.difference(&observed).cloned().collect();
        stats.parsed.sort();
        stats.reused.sort();
        let mut changed: BTreeSet<_> = stats.parsed.iter().chain(&stats.evicted).cloned().collect();
        // 未修改源码也可能因新建精确扩展文件或 externals 变化改变导入目标。
        let importers: BTreeSet<_> = self
            .previous_graph
            .edges
            .iter()
            .chain(&program.module_graph.edges)
            .map(|edge| edge.importer.clone())
            .collect();
        for importer in importers {
            let old: Vec<_> = self
                .previous_graph
                .edges
                .iter()
                .filter(|edge| edge.importer == importer)
                .collect();
            let new: Vec<_> = program
                .module_graph
                .edges
                .iter()
                .filter(|edge| edge.importer == importer)
                .collect();
            if old != new {
                changed.insert(importer);
            }
        }
        stats.affected = affected_modules(&self.previous_graph, &program.module_graph, changed);
        self.previous_graph = program.module_graph.clone();
        if !program.has_errors() {
            self.last_successful = Some(program.clone());
        }
        Ok(AnalysisUpdate { program, stats })
    }
}
