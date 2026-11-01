//! 模块绑定通过实际多文件源码验证，不执行 JavaScript。

use kengine::*;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
struct Project(PathBuf);
impl Project {
    fn new(files: &[(&str, &str)]) -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let result = Self(
            std::env::temp_dir().join(format!("kengine-binding-{}-{stamp}", std::process::id())),
        );
        fs::create_dir(&result.0)?;
        for (name, source) in files {
            fs::write(result.0.join(name), source)?
        }
        Ok(result)
    }
    fn analyze(&self) -> Result<ProgramIR> {
        Ok(Engine::new().analyze_project(
            &self.0,
            AnalyzeOptions {
                entry_points: vec!["main.ts".into()],
                ..Default::default()
            },
        )?)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn imported<'a>(
    program: &'a ProgramIR,
    name: &str,
    namespace: SymbolNamespace,
) -> Result<&'a LinkedImportIR> {
    let module = program
        .modules
        .iter()
        .find(|module| module.path.ends_with("main.ts"))
        .ok_or("缺少入口")?;
    let symbol = module
        .semantic
        .symbols
        .iter()
        .find(|symbol| symbol.name == name && symbol.namespace == namespace)
        .ok_or("缺少导入符号")?;
    program
        .module_bindings
        .imports
        .iter()
        .find(|binding| binding.module == module.path && binding.symbol == symbol.id)
        .ok_or_else(|| "缺少跨模块绑定".into())
}

#[test]
fn named_default_namespace_and_type_imports_resolve_to_original_entities() -> Result {
    let project = Project::new(&[
        (
            "main.ts",
            "import Default, { renamed, type Kind, Thing } from './barrel'; import * as all from './barrel'; export { renamed as publicValue }; export const result = Default(renamed);",
        ),
        (
            "barrel.ts",
            "export { value as renamed, default, type Kind, Thing } from './source';",
        ),
        (
            "source.ts",
            "export const value = 7; export default function compute(input: number) { return input; } export interface Kind { tag: string } export class Thing {}",
        ),
    ])?;
    let program = project.analyze()?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    for name in ["Default", "renamed", "Thing"] {
        assert!(
            matches!(imported(&program, name, SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::Symbol { module, .. }] if module.ends_with("source.ts"))
        );
    }
    assert!(
        matches!(imported(&program, "Kind", SymbolNamespace::Type)?.targets.as_slice(), [ModuleBindingTarget::Symbol { module, .. }] if module.ends_with("source.ts"))
    );
    assert!(
        matches!(imported(&program, "all", SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::Namespace { module }] if module.ends_with("barrel.ts"))
    );
    let exported = program
        .module_bindings
        .exports
        .iter()
        .find(|item| {
            item.module.ends_with("main.ts")
                && item.name == "publicValue"
                && item.namespace == SymbolNamespace::Value
        })
        .ok_or("缺少经导入转导的导出")?;
    assert_eq!(
        exported.targets,
        imported(&program, "renamed", SymbolNamespace::Value)?.targets
    );
    assert_eq!(
        program,
        serde_json::from_str::<ProgramIR>(&serde_json::to_string(&program)?)?
    );
    Ok(())
}

#[test]
fn export_star_cycles_converge_and_duplicate_paths_to_one_symbol_are_not_ambiguous() -> Result {
    let project = Project::new(&[
        (
            "main.ts",
            "import { value, next } from './a'; export { value };",
        ),
        ("a.ts", "export * from './b'; export * from './source';"),
        (
            "b.ts",
            "export * from './a'; export * from './source'; export const next = 2;",
        ),
        ("source.ts", "export const value = 1; export default 8;"),
    ])?;
    let program = project.analyze()?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    assert_eq!(program.module_graph.cycles.len(), 1);
    assert_eq!(
        imported(&program, "value", SymbolNamespace::Value)?
            .targets
            .len(),
        1
    );
    assert!(
        !program
            .module_bindings
            .exports
            .iter()
            .any(|item| item.module.ends_with("a.ts") && item.name == "default")
    );
    Ok(())
}

#[test]
fn ambiguous_star_export_is_rejected_when_imported_and_explicit_export_overrides() -> Result {
    let project = Project::new(&[
        (
            "main.ts",
            "import { value } from './barrel'; export const copy = value;",
        ),
        (
            "barrel.ts",
            "export * from './left'; export * from './right';",
        ),
        ("left.ts", "export const value = 1;"),
        ("right.ts", "export const value = 2;"),
    ])?;
    let ambiguous = project.analyze()?;
    assert!(
        ambiguous
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KE2102")
    );
    assert_eq!(
        imported(&ambiguous, "value", SymbolNamespace::Value)?
            .targets
            .len(),
        2
    );
    fs::write(
        project.0.join("main.ts"),
        "export { value } from './barrel';",
    )?;
    let reexport = project.analyze()?;
    assert!(
        reexport
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KE2102")
    );
    fs::write(
        project.0.join("main.ts"),
        "import { value } from './barrel'; export const copy = value;",
    )?;
    fs::write(
        project.0.join("barrel.ts"),
        "export * from './left'; export * from './right'; export { value } from './left';",
    )?;
    let explicit = project.analyze()?;
    assert!(!explicit.has_errors(), "{:?}", explicit.diagnostics);
    assert!(
        matches!(imported(&explicit, "value", SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::Symbol { module, .. }] if module.ends_with("left.ts"))
    );
    Ok(())
}

#[test]
fn missing_names_type_only_restrictions_and_missing_local_exports_have_diagnostics() -> Result {
    for (main, source) in [
        (
            "import { missing } from './source';",
            "export const value = 1;",
        ),
        (
            "import type { missing } from './source';",
            "export const value = 1;",
        ),
        ("export { absent };", "export const value = 1;"),
        (
            "export { missing } from './source';",
            "export const value = 1;",
        ),
    ] {
        let project = Project::new(&[("main.ts", main), ("source.ts", source)])?;
        let program = project.analyze()?;
        assert!(program.has_errors(), "{main}");
        assert!(
            program
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "KE2101" && diagnostic.span.is_some()),
            "{:?}",
            program.diagnostics
        );
    }
    Ok(())
}

#[test]
fn expression_defaults_and_namespace_reexports_keep_non_symbol_targets() -> Result {
    let project = Project::new(&[
        (
            "main.ts",
            "import value from './source'; import { group } from './barrel'; export default value;",
        ),
        ("barrel.ts", "export * as group from './source';"),
        ("source.ts", "export default 8;"),
    ])?;
    let program = project.analyze()?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    assert!(
        matches!(imported(&program, "value", SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::Expression { module, .. }] if module.ends_with("source.ts"))
    );
    assert!(
        matches!(imported(&program, "group", SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::Namespace { module }] if module.ends_with("source.ts"))
    );
    Ok(())
}

#[test]
fn explicit_externals_remain_unverified_and_external_stars_are_not_faked() -> Result {
    let project = Project::new(&[(
        "main.ts",
        "import Remote from 'remote'; export { Remote }; export * from 'other';",
    )])?;
    let program = Engine::new().analyze_project(
        &project.0,
        AnalyzeOptions {
            entry_points: vec!["main.ts".into()],
            resolve: ResolveOptions {
                externals: ["remote".into(), "other".into()].into(),
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    assert!(
        matches!(imported(&program, "Remote", SymbolNamespace::Value)?.targets.as_slice(), [ModuleBindingTarget::External { specifier, imported }] if specifier == "remote" && imported == "default")
    );
    assert_eq!(program.module_bindings.external_star_exports.len(), 1);
    assert_eq!(
        program.module_bindings.external_star_exports[0].specifier,
        "other"
    );
    Ok(())
}

#[test]
fn type_only_value_views_support_typeof_and_never_create_runtime_exports() -> Result {
    for barrel in [
        "export type { value } from './source';",
        "export type * from './source';",
        "import type { value } from './source'; export { value };",
        "import type { value } from './source'; export type { value };",
        "const value = 1; export type { value };",
    ] {
        let project = Project::new(&[
            (
                "main.ts",
                "import type { value } from './barrel'; type Shape = typeof value;",
            ),
            ("barrel.ts", barrel),
            ("source.ts", "export const value = 7;"),
        ])?;
        let program = project.analyze()?;
        assert!(!program.has_errors(), "{barrel}: {:?}", program.diagnostics);
        assert_eq!(
            imported(&program, "value", SymbolNamespace::Type)?
                .targets
                .len(),
            1
        );
        assert!(
            !program
                .module_bindings
                .exports
                .iter()
                .any(|item| item.module.ends_with("barrel.ts")
                    && item.name == "value"
                    && item.namespace == SymbolNamespace::Value),
            "type-only 不得开放运行期导出：{barrel}"
        );
        fs::write(
            project.0.join("main.ts"),
            "import { value } from './barrel'; type Shape = typeof value;",
        )?;
        let ordinary = project.analyze()?;
        assert!(
            imported(&ordinary, "value", SymbolNamespace::Value)?
                .targets
                .is_empty()
        );
        assert_eq!(
            imported(&ordinary, "value", SymbolNamespace::Type)?
                .targets
                .len(),
            1
        );
    }
    Ok(())
}

#[test]
fn longer_type_reexport_paths_take_priority_over_value_views() -> Result {
    let project = Project::new(&[
        ("main.ts", "import type { shared } from './barrel';"),
        (
            "barrel.ts",
            "export * from './value'; export type { shared } from './middle';",
        ),
        ("value.ts", "export const shared = 1;"),
        ("middle.ts", "export type { shared } from './type';"),
        ("type.ts", "export interface shared { item: string }"),
    ])?;
    let program = project.analyze()?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    assert!(
        matches!(imported(&program, "shared", SymbolNamespace::Type)?.targets.as_slice(), [ModuleBindingTarget::Symbol { module, .. }] if module.ends_with("type.ts"))
    );
    Ok(())
}
