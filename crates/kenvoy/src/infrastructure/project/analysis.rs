use std::{collections::BTreeSet, fs};

use kengine::{AnalysisSession, AnalysisUpdate, AnalyzeOptions, ResolveOptions, ResolvedModule};

use super::{
    Project,
    error::{ProjectError as Error, Result, io},
};

impl Project {
    pub fn analyze(&self, session: &mut AnalysisSession) -> Result<AnalysisUpdate> {
        let packages = self.external_packages()?;
        let mut options = AnalyzeOptions {
            entry_points: vec![self.entry.clone()],
            resolve: ResolveOptions {
                externals: packages.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        if let Some(previous) = session.last_successful() {
            let resolver = ResolveOptions {
                root: self.root.clone(),
                ..Default::default()
            };
            for edge in &previous.module_graph.edges {
                if matches!(edge.target, Some(ResolvedModule::External { .. })) {
                    let package = !edge.specifier.starts_with('.')
                        && packages.iter().any(|package| {
                            edge.specifier == *package
                                || edge
                                    .specifier
                                    .strip_prefix(package)
                                    .is_some_and(|tail| tail.starts_with('/'))
                        });
                    let stylesheet = edge.specifier.starts_with('.')
                        && matches!(kengine::Engine::new().resolve_module(&edge.importer, &edge.specifier, &resolver),
                        Ok(ResolvedModule::File { path }) if path.extension().is_some_and(|extension| extension == "css"));
                    if package || stylesheet {
                        options.resolve.externals.insert(edge.specifier.clone());
                    }
                }
            }
        }
        let initial_externals = options.resolve.externals.clone();
        let first = session.analyze_project(&self.root, options.clone())?;
        for edge in &first.program.module_graph.edges {
            let package = !edge.specifier.starts_with('.')
                && packages.iter().any(|package| {
                    edge.specifier == *package
                        || edge
                            .specifier
                            .strip_prefix(package)
                            .is_some_and(|tail| tail.starts_with('/'))
                });
            let stylesheet = matches!(&edge.target, Some(ResolvedModule::File { path }) if path.extension().is_some_and(|extension| extension == "css"));
            if package || stylesheet {
                options.resolve.externals.insert(edge.specifier.clone());
            }
        }
        if options.resolve.externals == initial_externals {
            self.validate_external_stylesheets(&first)?;
            return Ok(first);
        }
        // 第二遍复用第一遍缓存，不为 external 发现重新解析所有源码。
        let mut final_update = session.analyze_project(&self.root, options)?;
        final_update.stats.parsed.extend(first.stats.parsed);
        final_update.stats.affected.extend(first.stats.affected);
        final_update.stats.evicted.extend(first.stats.evicted);
        for paths in [
            &mut final_update.stats.parsed,
            &mut final_update.stats.affected,
            &mut final_update.stats.evicted,
        ] {
            paths.sort();
            paths.dedup();
        }
        final_update
            .stats
            .reused
            .retain(|path| !final_update.stats.parsed.contains(path));
        self.validate_external_stylesheets(&final_update)?;
        Ok(final_update)
    }

    /// 相对 external 名称只是一项解析提示。必须按每个 importer 重新校验，
    /// 避免一个目录下的合法 CSS 放行另一个目录中同名但缺失或越界的文件。
    fn validate_external_stylesheets(&self, update: &AnalysisUpdate) -> Result<()> {
        let resolver = ResolveOptions {
            root: self.root.clone(),
            ..Default::default()
        };
        let engine = kengine::Engine::new();
        for edge in &update.program.module_graph.edges {
            if !edge.specifier.starts_with('.')
                || !matches!(edge.target, Some(ResolvedModule::External { .. }))
            {
                continue;
            }
            let result = engine.resolve_module(&edge.importer, &edge.specifier, &resolver);
            if !matches!(result, Ok(ResolvedModule::File { ref path }) if path.extension().is_some_and(|extension| extension == "css"))
            {
                let reason = match result {
                    Err(error) => error.to_string(),
                    _ => "相对 external 只能指向有效 CSS 文件".into(),
                };
                let mut diagnostic = crate::domain::source::DslDiagnostic::new(
                    "KV3006",
                    crate::domain::source::SourceLocation {
                        path: edge.importer.clone(),
                        span: edge.span.clone(),
                    },
                    format!("样式模块不能加载：{reason}"),
                    "检查当前导入文件对应的 CSS 路径，样式文件必须存在于项目根内",
                );
                diagnostic.category = crate::domain::source::DslDiagnosticCategory::Module;
                return Err(Error::Dsl(vec![diagnostic]));
            }
        }
        Ok(())
    }

    /// npm 和样式解析是宿主责任，KEngine 只收到精确 external 名称。
    /// 此处只建立允许集合，是否真正可打包在构建阶段验证。
    pub(crate) fn external_packages(&self) -> Result<BTreeSet<String>> {
        let mut packages: BTreeSet<String> =
            ["@kenvoy/client", "react", "react-dom", "react-router"]
                .into_iter()
                .map(str::to_owned)
                .collect();
        let package_path = self.root.join("package.json");
        if package_path.is_file() {
            let data = fs::read(&package_path).map_err(|error| io(&package_path, error))?;
            let manifest: serde_json::Value = serde_json::from_slice(&data)?;
            for key in ["dependencies", "devDependencies", "peerDependencies"] {
                if let Some(entries) = manifest.get(key).and_then(serde_json::Value::as_object) {
                    for name in entries.keys() {
                        if !valid_package_name(name) {
                            return Err(Error::InvalidConfig(format!(
                                "package.json 中的包名无效：{name}"
                            )));
                        }
                        packages.insert(name.clone());
                    }
                }
            }
        }
        Ok(packages)
    }
}

fn valid_package_name(name: &str) -> bool {
    fn segment(value: &str) -> bool {
        !value.is_empty()
            && !value.starts_with(['.', '_'])
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    }
    if let Some(scoped) = name.strip_prefix('@') {
        scoped
            .split_once('/')
            .is_some_and(|(scope, package)| segment(scope) && segment(package))
    } else {
        segment(name)
    }
}
