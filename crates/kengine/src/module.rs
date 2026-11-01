use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::SourceSpan;

/// 第一版仅解析源码文件及显式 external，不实现 npm 条件导出或 tsconfig aliases。
#[derive(Debug, Clone)]
pub struct ResolveOptions {
    pub root: PathBuf,
    /// 精确匹配的外部模块说明符，不隐式开放包的所有子路径。
    pub externals: BTreeSet<String>,
    pub map_js_to_ts: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            externals: BTreeSet::new(),
            map_js_to_ts: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedModule {
    File { path: PathBuf },
    External { specifier: String },
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("模块 {specifier} 未能从 {importer} 解析到源码文件")]
    NotFound {
        importer: PathBuf,
        specifier: String,
    },
    #[error("模块路径 {path} 超出分析根目录 {root}")]
    OutsideRoot { path: PathBuf, root: PathBuf },
    #[error("不支持模块说明符 {specifier}；相对路径或显式 external 才可解析")]
    UnsupportedSpecifier { specifier: String },
    #[error("定位模块路径 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub(crate) fn resolve(
    importer: &Path,
    specifier: &str,
    options: &ResolveOptions,
) -> Result<ResolvedModule, ResolveError> {
    if options.externals.contains(specifier) {
        return Ok(ResolvedModule::External {
            specifier: specifier.into(),
        });
    }
    if (!specifier.starts_with("./") && !specifier.starts_with("../"))
        || specifier.contains(['\\', '\0', ':'])
    {
        return Err(ResolveError::UnsupportedSpecifier {
            specifier: specifier.into(),
        });
    }
    let root = options
        .root
        .canonicalize()
        .map_err(|source| ResolveError::Io {
            path: options.root.clone(),
            source,
        })?;
    let importer = if importer.is_absolute() {
        importer.to_path_buf()
    } else {
        root.join(importer)
    };
    let base = importer.parent().unwrap_or(&root).join(specifier);
    let mut candidates = vec![base.clone()];
    let extension = base.extension().and_then(|extension| extension.to_str());
    if options.map_js_to_ts && matches!(extension, Some("js" | "jsx" | "mjs" | "cjs")) {
        for extension in match extension {
            Some("mjs") => &["mts"][..],
            Some("cjs") => &["cts"][..],
            _ => &["ts", "tsx"][..],
        } {
            candidates.push(base.with_extension(extension));
        }
    }
    if !matches!(
        extension,
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs")
    ) {
        for extension in ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
            let mut appended = base.as_os_str().to_os_string();
            appended.push(format!(".{extension}"));
            candidates.push(PathBuf::from(appended));
        }
    }
    for extension in ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
        candidates.push(base.join(format!("index.{extension}")));
    }
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let path = candidate
            .canonicalize()
            .map_err(|source| ResolveError::Io {
                path: candidate,
                source,
            })?;
        if !path.starts_with(&root) {
            return Err(ResolveError::OutsideRoot { path, root });
        }
        return Ok(ResolvedModule::File { path });
    }
    Err(ResolveError::NotFound {
        importer,
        specifier: specifier.into(),
    })
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
