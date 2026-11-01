//! 使用 KEngine 公共内存解析 API 读取真实 ESM 依赖，不扫描注释或普通字符串。

use super::{layout::Source, rust_syntax::normalize};
use crate::Result;
use kengine::{Engine, LiteralIR, ModuleIR, ParseOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const FRONTEND: &str = "crates/kenvoy/frontend/src";

enum Dependency {
    Source(PathBuf),
    External(String),
}

pub(super) fn inspect(sources: &[Source], errors: &mut Vec<String>) -> Result<usize> {
    let known: BTreeSet<_> = sources.iter().map(|source| source.path.clone()).collect();
    let engine = Engine::new();
    let mut graph = BTreeMap::new();
    for source in sources.iter().filter(|source| {
        source.path.starts_with(FRONTEND)
            && matches!(
                source.path.extension().and_then(|value| value.to_str()),
                Some("ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs")
            )
    }) {
        let parsed = engine.parse_source(&source.path, &source.text, ParseOptions::default())?;
        if parsed.has_errors() {
            return Err(format!(
                "前端源码解析失败：{}：{:?}",
                source.path.display(),
                parsed.diagnostics
            )
            .into());
        }
        let module = parsed.module.ok_or("前端源码解析未返回模块")?;
        let mut dependencies = Vec::new();
        for specifier in specifiers(&module)? {
            let dependency = if specifier.starts_with('.') {
                match resolve(&source.path, &specifier, &known) {
                    Some(target) => Dependency::Source(target),
                    None => {
                        errors.push(format!(
                            "前端相对依赖未解析：{} -> {specifier}",
                            source.path.display()
                        ));
                        continue;
                    }
                }
            } else if specifier == "@kenvoy/client" {
                Dependency::Source(Path::new(FRONTEND).join("sdk.ts"))
            } else {
                Dependency::External(specifier)
            };
            check(&source.path, &dependency, errors);
            dependencies.push(dependency);
        }
        graph.insert(source.path.clone(), dependencies);
    }
    for path in graph.keys().filter(|path| {
        path.as_path() == Path::new(FRONTEND).join("sdk.ts")
            || path.file_name().is_some_and(|name| name == "context.ts")
    }) {
        let mut visited = BTreeSet::new();
        let mut pending = vec![path.clone()];
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            for dependency in graph.get(&current).into_iter().flatten() {
                match dependency {
                    Dependency::External(name) if name == "virtual:kenvoy-registry" => {
                        errors.push(format!(
                            "SDK/Context 不得回引虚拟组件注册表：{} -> {}",
                            path.display(),
                            current.display()
                        ));
                    }
                    Dependency::Source(target) => {
                        if target.file_stem().is_some_and(|name| name == "RuntimePage") {
                            errors.push(format!(
                                "SDK/Context 不得回引页面解释器：{} -> {}",
                                path.display(),
                                target.display()
                            ));
                        }
                        pending.push(target.clone());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(graph.len())
}

fn specifiers(module: &ModuleIR) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for source in module
        .imports
        .iter()
        .filter_map(|item| item.source.as_ref())
        .chain(
            module
                .exports
                .iter()
                .filter_map(|item| item.source.as_ref()),
        )
    {
        result.push(source.to_string()?);
    }
    // 通用 IR 保留 Call 与字符串字面量的精确 span，动态 import 只在已解析 Call 内识别。
    for expression in module
        .expressions
        .iter()
        .filter(|expression| expression.kind == "Call")
    {
        let raw = module
            .raw(&expression.span)
            .ok_or("前端调用表达式位置无效")?;
        let Some(rest) = trivia(raw).strip_prefix("import") else {
            continue;
        };
        let Some(argument) = trivia(rest).strip_prefix('(') else {
            continue;
        };
        let argument = trivia(argument);
        let start = expression.span.end - argument.len();
        let literal = module.expressions.iter().find(|item| {
            item.span.start == start && matches!(item.literal, Some(LiteralIR::String(_)))
        });
        let Some(literal) = literal else {
            continue;
        };
        let tail = trivia(
            module
                .source
                .get(literal.span.end..expression.span.end)
                .ok_or("前端导入位置无效")?,
        );
        if !tail.starts_with([')', ',']) {
            continue;
        }
        if let Some(LiteralIR::String(value)) = &literal.literal {
            result.push(value.to_string()?);
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn trivia(mut source: &str) -> &str {
    loop {
        source = source.trim_start();
        if let Some(comment) = source.strip_prefix("/*") {
            if let Some(end) = comment.find("*/") {
                source = &comment[end + 2..];
                continue;
            }
        } else if let Some(comment) = source.strip_prefix("//") {
            source = comment
                .find(['\r', '\n'])
                .map(|end| &comment[end..])
                .unwrap_or("");
            continue;
        }
        return source;
    }
}

fn resolve(importer: &Path, specifier: &str, known: &BTreeSet<PathBuf>) -> Option<PathBuf> {
    let base = normalize(&importer.parent()?.join(specifier));
    if known.contains(&base) {
        return Some(base);
    }
    let mapped = match base.extension().and_then(|value| value.to_str()) {
        Some("js" | "jsx") => &["ts", "tsx"][..],
        Some("mjs") => &["mts"][..],
        Some("cjs") => &["cts"][..],
        _ => &[],
    };
    for extension in mapped {
        let candidate = base.with_extension(extension);
        if known.contains(&candidate) {
            return Some(candidate);
        }
    }
    for extension in ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "css"] {
        let mut appended = base.as_os_str().to_os_string();
        appended.push(format!(".{extension}"));
        let candidate = PathBuf::from(appended);
        if known.contains(&candidate) {
            return Some(candidate);
        }
        let candidate = base.join(format!("index.{extension}"));
        if known.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn layer(path: &Path) -> Option<&str> {
    path.strip_prefix(FRONTEND).ok()?.iter().next()?.to_str()
}

fn check(source: &Path, dependency: &Dependency, errors: &mut Vec<String>) {
    let source_layer = layer(source).unwrap_or("");
    let violation = match dependency {
        Dependency::Source(target) => {
            let target_layer = layer(target).unwrap_or("");
            if !target.starts_with(FRONTEND) {
                return errors.push(format!(
                    "前端源码不得回引源码根之外的模块：{} -> {}",
                    source.display(),
                    target.display()
                ));
            }
            match source_layer {
                "domain" => [
                    "application",
                    "infrastructure",
                    "presentation",
                    "app",
                    "sdk.ts",
                    "main.tsx",
                ]
                .contains(&target_layer),
                "application" => [
                    "infrastructure",
                    "presentation",
                    "app",
                    "sdk.ts",
                    "main.tsx",
                ]
                .contains(&target_layer),
                "infrastructure" => {
                    ["presentation", "app", "sdk.ts", "main.tsx"].contains(&target_layer)
                }
                "presentation" => target_layer == "app",
                _ => false,
            }
        }
        Dependency::External(name) if ["domain", "application"].contains(&source_layer) => {
            frontend_driver(name)
        }
        _ => false,
    };
    if violation {
        let target = match dependency {
            Dependency::Source(path) => path.display().to_string(),
            Dependency::External(name) => name.clone(),
        };
        errors.push(format!("前端层依赖越界：{} -> {target}", source.display()));
    }
}

fn frontend_driver(name: &str) -> bool {
    name.starts_with("node:")
        || name == "virtual:kenvoy-registry"
        || ["react", "react-dom", "react-router-dom", "@mui"]
            .iter()
            .any(|prefix| {
                name == *prefix
                    || name
                        .strip_prefix(*prefix)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
}
