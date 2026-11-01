use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use super::{
    Result,
    error::{invalid, io},
};
use crate::domain::model::{ApplicationIR, ModuleReferenceKind};
use kengine::{DependencyKind, ProgramIR, ResolvedModule};

pub(super) fn verify_sources(program: &ProgramIR, root: &Path) -> Result<()> {
    for module in &program.modules {
        let path = module
            .path
            .canonicalize()
            .map_err(|error| io(&module.path, error))?;
        if !path.starts_with(root) {
            return Err(invalid("分析模块越过项目根目录"));
        }
        if fs::read(&path).map_err(|error| io(&path, error))? != module.source.as_bytes() {
            return Err(invalid("分析后源码已改变，请重新分析再构建"));
        }
    }
    Ok(())
}

pub(super) fn private_modules(program: &ProgramIR, app: &ApplicationIR) -> BTreeSet<PathBuf> {
    let shared: BTreeSet<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::SharedFunction)
        .map(|reference| &reference.module)
        .collect();
    let server_roots: BTreeSet<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
        .map(|reference| reference.module.clone())
        .collect();
    let mut queue: VecDeque<_> = app
        .module_references
        .iter()
        .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
        .map(|reference| reference.module.clone())
        .chain(
            program
                .modules
                .iter()
                .filter(|module| module.path.to_string_lossy().contains(".server."))
                .map(|module| module.path.clone()),
        )
        .collect();
    let mut result = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        let name = path.to_string_lossy();
        if !server_roots.contains(&path)
            && !name.contains(".server.")
            && (shared.contains(&path) || name.contains(".shared."))
        {
            continue;
        }
        if !result.insert(path.clone()) {
            continue;
        }
        for edge in program.module_graph.edges.iter().filter(|edge| {
            edge.importer == path
                && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
        }) {
            if let Some(ResolvedModule::File { path }) = &edge.target {
                queue.push_back(path.clone());
            }
        }
    }
    result
}
