use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
};

use kengine::{DependencyKind, ResolvedModule};

use crate::{
    application::compilation::client_slice::{self, ClientCodeTransformer, ClientImportTarget},
    domain::{
        model::ModuleReferenceKind,
        source::{DslDiagnostic, DslDiagnosticCategory, SourceLocation},
    },
};

use super::Analyzer;

impl<T: ClientCodeTransformer> Analyzer<'_, T> {
    pub(super) fn check_boundaries(&mut self) {
        let server_roots: BTreeSet<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
            .map(|reference| reference.module.clone())
            .collect();
        let shared_modules: BTreeSet<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::SharedFunction)
            .map(|reference| reference.module.clone())
            .collect();
        let mut private_modules = BTreeSet::new();
        let mut private_queue: VecDeque<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
            .map(|reference| reference.module.clone())
            .chain(
                self.program
                    .modules
                    .iter()
                    .filter(|module| is_server_path(&module.path))
                    .map(|module| module.path.clone()),
            )
            .collect();
        while let Some(path) = private_queue.pop_front() {
            if !server_roots.contains(&path)
                && !is_server_path(&path)
                && (shared_modules.contains(&path)
                    || path
                        .to_string_lossy()
                        .split(['/', '\\'])
                        .any(|part| part.contains(".shared.")))
            {
                continue;
            }
            if !private_modules.insert(path.clone()) {
                continue;
            }
            for edge in self.program.module_graph.edges.iter().filter(|edge| {
                edge.importer == path
                    && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
            }) {
                if let Some(ResolvedModule::File { path }) = &edge.target {
                    private_queue.push_back(path.clone());
                }
            }
        }
        for reference in &self.modules {
            if !matches!(
                reference.kind,
                ModuleReferenceKind::ClientComponent
                    | ModuleReferenceKind::ClientFunction
                    | ModuleReferenceKind::ClientExpression
                    | ModuleReferenceKind::SharedFunction
            ) {
                continue;
            }
            if reference.external {
                if is_server_path(&reference.module)
                    || reference.module.to_string_lossy().starts_with("node:")
                {
                    self.diagnostics.push(boundary(
                        reference.source.clone(),
                        "客户端不允许导入 Node 或服务端包路径",
                    ));
                }
                continue;
            }
            let sliced = reference.kind == ModuleReferenceKind::ClientExpression
                || reference.module == self.entry_source.path
                || reference.module.to_string_lossy().contains(".diu.");
            if server_roots.contains(&reference.module) {
                self.diagnostics.push(boundary(
                    reference.source.clone(),
                    "同一个模块不能同时注册服务端私有函数和浏览器或 Shared 函数",
                ));
                continue;
            }
            let mut queue = if sliced {
                match client_slice::slice(self.program, reference, self.transformer) {
                    Ok(slice) => {
                        let mut queue = VecDeque::new();
                        for target in slice.imports.into_values() {
                            match target {
                                ClientImportTarget::File(path) => queue.push_back(path),
                                ClientImportTarget::External(name)
                                    if name.starts_with("node:")
                                        || is_server_path(Path::new(&name)) =>
                                {
                                    self.diagnostics.push(boundary(
                                        reference.source.clone(),
                                        "客户端切片依赖包含服务端模块",
                                    ))
                                }
                                _ => {}
                            }
                        }
                        queue
                    }
                    Err(error) => {
                        self.diagnostics
                            .push(boundary(reference.source.clone(), error.message));
                        continue;
                    }
                }
            } else {
                VecDeque::from([reference.module.clone()])
            };
            let mut visited = BTreeSet::new();
            while let Some(path) = queue.pop_front() {
                if !visited.insert(path.clone()) {
                    continue;
                }
                if private_modules.contains(&path) || is_server_path(&path) {
                    let mut failure = boundary(
                        reference.source.clone(),
                        format!("客户端引用可到达服务端私有模块 {}", path.display()),
                    );
                    if let Some(module) = self
                        .program
                        .modules
                        .iter()
                        .find(|module| module.path == path)
                        && let Some(export) = module.exports.first()
                    {
                        failure.related.push(SourceLocation {
                            path: path.clone(),
                            span: export.span.clone(),
                        });
                    }
                    self.diagnostics.push(failure);
                    break;
                }
                if let Some(module) = self
                    .program
                    .modules
                    .iter()
                    .find(|module| module.path == path)
                {
                    // 动态 import 和 require 尚无可证明安全的闭包，禁止它们绕过静态图。
                    if module.expressions.iter().any(|expr| {
                        module.raw(&expr.span).is_some_and(|raw| {
                            (expr.kind == "Call" && dynamic_import(raw))
                                || raw.trim_start().starts_with("import.meta")
                        })
                    }) || module.semantic.references.iter().any(|reference| {
                        reference.name == "require"
                            && reference.namespace == kengine::SymbolNamespace::Value
                            && reference.symbol.is_none()
                    }) {
                        self.diagnostics.push(boundary(
                            reference.source.clone(),
                            "客户端模块含未建模的动态模块加载",
                        ));
                        break;
                    }
                }
                for edge in self.program.module_graph.edges.iter().filter(|edge| {
                    edge.importer == path
                        && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
                }) {
                    match &edge.target {
                        Some(ResolvedModule::File { path }) => queue.push_back(path.clone()),
                        Some(ResolvedModule::External { specifier })
                            if specifier.starts_with("node:")
                                || is_server_path(Path::new(specifier)) =>
                        {
                            self.diagnostics.push(boundary(
                                reference.source.clone(),
                                "客户端依赖不能包含 Node 或服务端私有模块",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn boundary(source: SourceLocation, message: impl Into<String>) -> DslDiagnostic {
    let mut error = DslDiagnostic::new(
        "KV2101",
        source,
        message,
        "拆分独立的 client/shared/server 模块，并只注册允许进入浏览器的导出",
    );
    error.category = DslDiagnosticCategory::Boundary;
    error
}

fn is_server_path(path: &Path) -> bool {
    path.to_string_lossy()
        .split(['/', '\\'])
        .any(|part| part.contains(".server."))
}

fn dynamic_import(raw: &str) -> bool {
    raw.trim_start()
        .strip_prefix("import")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '/'))
}
