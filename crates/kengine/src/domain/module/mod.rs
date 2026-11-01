pub(crate) mod resolution;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::domain::source::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedModule {
    File { path: PathBuf },
    External { specifier: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyKind {
    Import,
    TypeImport,
    Reexport,
    TypeReexport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEdge {
    pub importer: PathBuf,
    pub specifier: String,
    pub span: SourceSpan,
    pub kind: DependencyKind,
    /// 可无损表示为字符串的说明符即使解析失败也保留边，原因写入诊断。
    /// 无效 UTF-16 说明符仅保留在 ImportIR/ExportIR 和诊断中，错误图可能不完整。
    pub target: Option<ResolvedModule>,
}

/// 静态 import/re-export 依赖图；动态 import 保留在表达式中，不假定为静态依赖。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleGraph {
    pub nodes: Vec<PathBuf>,
    pub edges: Vec<ModuleEdge>,
    /// 含环的强连通分量，含自环；不把合法 ESM 循环默认判为错误。
    pub cycles: Vec<Vec<PathBuf>>,
}

impl ModuleGraph {
    pub(crate) fn update_cycles(&mut self) {
        let mut outgoing: BTreeMap<_, Vec<_>> = self
            .nodes
            .iter()
            .map(|path| (path.clone(), vec![]))
            .collect();
        let mut incoming = outgoing.clone();
        for edge in &self.edges {
            if let Some(ResolvedModule::File { path }) = &edge.target {
                outgoing
                    .entry(edge.importer.clone())
                    .or_default()
                    .push(path.clone());
                incoming
                    .entry(path.clone())
                    .or_default()
                    .push(edge.importer.clone());
            }
        }
        let mut visited = BTreeSet::new();
        let mut order = vec![];
        for start in outgoing.keys() {
            let mut stack = vec![(start.clone(), false)];
            while let Some((node, finishing)) = stack.pop() {
                if finishing {
                    order.push(node);
                    continue;
                }
                if !visited.insert(node.clone()) {
                    continue;
                }
                stack.push((node.clone(), true));
                if let Some(next) = outgoing.get(&node) {
                    for target in next.iter().rev() {
                        if !visited.contains(target) {
                            stack.push((target.clone(), false));
                        }
                    }
                }
            }
        }
        visited.clear();
        let mut cycles = vec![];
        for start in order.into_iter().rev() {
            if visited.contains(&start) {
                continue;
            }
            let mut component = vec![];
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                if !visited.insert(node.clone()) {
                    continue;
                }
                component.push(node.clone());
                if let Some(next) = incoming.get(&node) {
                    stack.extend(next.iter().cloned());
                }
            }
            component.sort();
            if component.len() > 1
                || component.first().is_some_and(|node| {
                    outgoing
                        .get(node)
                        .is_some_and(|targets| targets.contains(node))
                })
            {
                cycles.push(component);
            }
        }
        cycles.sort();
        self.cycles = cycles;
    }
}

pub(crate) fn affected_modules(
    old: &ModuleGraph,
    new: &ModuleGraph,
    changed: BTreeSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut reverse: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    for edge in old.edges.iter().chain(&new.edges) {
        if let Some(ResolvedModule::File { path }) = &edge.target {
            reverse
                .entry(path.clone())
                .or_default()
                .insert(edge.importer.clone());
        }
    }
    let mut affected = BTreeSet::new();
    let mut pending: Vec<_> = changed.into_iter().collect();
    while let Some(path) = pending.pop() {
        if affected.insert(path.clone())
            && let Some(importers) = reverse.get(&path)
        {
            pending.extend(importers.iter().cloned());
        }
    }
    affected.into_iter().collect()
}
