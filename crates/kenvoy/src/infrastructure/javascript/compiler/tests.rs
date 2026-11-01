//! 服务端编译与构建产物完整性测试。
use super::parser::restricted_parse;
use super::*;
use crate::{
    application::execution::{
        BUNDLE_VERSION, FunctionTarget, PreparedBundle, PreparedModule, Trust,
    },
    domain::{
        model::{APPLICATION_IR_VERSION, ApplicationIR, ModuleReference, ModuleReferenceKind},
        source::SourceLocation,
    },
};
use kengine::{AnalyzeOptions, Engine, ProgramIR, ResolveOptions};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
static COUNTER: AtomicUsize = AtomicUsize::new(0);
struct Project(PathBuf);
impl Project {
    fn new(files: &[(&str, &str)]) -> TestResult<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "kenvoy-js-{}-{stamp}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        for (name, source) in files {
            fs::write(root.join(name), source)?;
        }
        Ok(Self(root))
    }
    fn analyze(&self) -> TestResult<(ProgramIR, ApplicationIR)> {
        let program = Engine::new().analyze_project(
            &self.0,
            AnalyzeOptions {
                entry_points: vec!["main.ts".into()],
                resolve: ResolveOptions {
                    externals: ["react".into()].into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        assert!(!program.has_errors(), "{:?}", program.diagnostics);
        let path = self.0.join("main.ts").canonicalize()?;
        let module = program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or("没有入口模块")?;
        let source = SourceLocation {
            path: path.clone(),
            span: module.exports.first().ok_or("没有入口导出")?.span.clone(),
        };
        let application = ApplicationIR {
            format_version: APPLICATION_IR_VERSION,
            name: "计算测试".into(),
            component_overrides: Default::default(),
            resources: vec![],
            pages: vec![],
            actions: vec![],
            navigation: vec![],
            permissions: vec![],
            module_references: vec![ModuleReference {
                id: "module_1".into(),
                module: path,
                export: "run".into(),
                external: false,
                kind: ModuleReferenceKind::ServerFunction,
                expression: None,
                source,
            }],
        };
        Ok((program, application))
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn bundle(code: &str) -> PreparedBundle {
    PreparedBundle {
        format_version: BUNDLE_VERSION,
        modules: [(
            "m_1".into(),
            PreparedModule {
                code: code.into(),
                imports: BTreeMap::new(),
            },
        )]
        .into(),
        functions: [(
            "module_1".into(),
            FunctionTarget {
                module: "m_1".into(),
                export: "run".into(),
            },
        )]
        .into(),
    }
}

#[test]
fn ts_static_modules_reexports_async_and_type_only_imports_prepare_without_execution() -> TestResult
{
    let project = Project::new(&[
        ("main.ts", "export { calculate as run } from './barrel';"),
        ("barrel.ts", "import './effect'; export * from './math';"),
        ("effect.ts", "throw new Error('编译阶段不得执行');"),
        (
            "math.ts",
            "import type { Input } from './types'; export async function calculate<T extends number>(value:T): Promise<number> { return value * 2; }",
        ),
        ("types.ts", "export interface Input { amount:number }"),
    ])?;
    let (program, application) = project.analyze()?;
    let prepared = prepare(&program, &application, Trust::Project)?;
    assert_eq!(
        prepared.modules.len(),
        4,
        "只保留运行时静态依赖；保留门面副作用模块"
    );
    assert_eq!(prepared.functions.len(), 1);
    assert!(
        prepared
            .modules
            .values()
            .any(|module| module.code.contains("async function calculate"))
    );
    assert!(prepared.modules.values().all(
        |module| !module.code.contains("interface") && !module.code.contains("extends number")
    ));
    assert!(!serde_json::to_string(&prepared)?.contains(&project.0.to_string_lossy().to_string()));
    validate(&serde_json::from_slice(&serde_json::to_vec(&prepared)?)?)?;
    assert!(prepare(&program, &application, Trust::Untrusted).is_err());
    Ok(())
}

#[test]
fn module_cycles_terminate_and_static_imports_remain_mapped() -> TestResult {
    let project = Project::new(&[
        (
            "main.ts",
            "import { twice } from './math'; export function run(v:number){return twice(v)}; export const base=2;",
        ),
        (
            "math.ts",
            "import { base } from './main'; export function twice(v:number){return v*base}",
        ),
    ])?;
    let (program, application) = project.analyze()?;
    let prepared = prepare(&program, &application, Trust::Project)?;
    assert_eq!(prepared.modules.len(), 2);
    assert!(
        prepared
            .modules
            .values()
            .all(|module| module.imports.len() == 1)
    );
    Ok(())
}

#[test]
fn ordinary_typescript_generic_arrows_are_not_parsed_as_tsx() -> TestResult {
    let project = Project::new(&[("main.ts", "export const run=<T>(value:T):T=>value;")])?;
    let (program, application) = project.analyze()?;
    let prepared = prepare(&program, &application, Trust::Project)?;
    assert!(
        prepared
            .modules
            .values()
            .any(|module| module.code.contains("(value)=>value"))
    );
    Ok(())
}

#[test]
fn source_capability_checks_reject_dynamic_loading_jsx_and_legacy_modules() {
    for source in [
        "export async function run(){return import /* 空白 */ ('./x')}",
        "export function run(){return require('./x')}",
        "export function run(){return import.meta.url}",
        "export function run(){return <section/>}",
        "export function run(){return <>内容</>}",
        "import x from './x' with {type:'json'}; export const run=()=>x;",
        "export { run } from './x' with {type:'json'};",
        "import x = require('./x'); export const run=()=>x;",
        "const run=()=>1; export = run;",
        "export function run(){return (0,eval)('1')}",
        "export const run=()=>new Function('return 1')();",
    ] {
        let error = restricted_parse(source, "source.tsx", true).expect_err(source);
        assert_eq!(error.code, "KV6003", "{source}");
        let source = error.source.expect("诊断应包含源码位置");
        assert_eq!(source.path, PathBuf::from("source.tsx"));
        assert!(source.span.end > source.span.start);
    }
}

#[test]
fn external_packages_are_not_executable_server_dependencies() -> TestResult {
    let project = Project::new(&[(
        "main.ts",
        "import {createElement} from 'react'; export const run=()=>createElement('span');",
    )])?;
    let (program, application) = project.analyze()?;
    assert!(prepare(&program, &application, Trust::Project).is_err());
    Ok(())
}

#[test]
fn client_transpilation_preserves_jsx_but_removes_types() -> TestResult {
    let output = SwcClientCodeTransformer.transpile(
        "export function Card<T>({value}:{value:T}) { return <div>{String(value)}</div> }",
        "Card.tsx",
        true,
    )?;
    assert!(output.contains("<div>"));
    assert!(!output.contains("value:T") && !output.contains("<T>"));
    assert!(
        SwcClientCodeTransformer
            .transpile("export const x: =", "invalid.ts", false)
            .is_err()
    );
    Ok(())
}

#[test]
fn artifact_validation_rejects_version_ids_maps_exports_syntax_and_capability_tampering()
-> TestResult {
    let valid = bundle("export const run=(v)=>v*2;");
    validate(&valid)?;
    let mut invalid = valid.clone();
    invalid.format_version += 1;
    assert!(validate(&invalid).is_err());
    let mut invalid = valid.clone();
    invalid
        .functions
        .get_mut("module_1")
        .ok_or("函数缺失")?
        .export = "missing".into();
    assert!(validate(&invalid).is_err());
    let mut invalid = valid.clone();
    invalid.modules.insert(
        "../secret.ts".into(),
        PreparedModule {
            code: "".into(),
            imports: BTreeMap::new(),
        },
    );
    assert!(validate(&invalid).is_err());
    let mut invalid = valid.clone();
    invalid
        .modules
        .get_mut("m_1")
        .ok_or("模块缺失")?
        .imports
        .insert("./missing".into(), "m_2".into());
    assert!(validate(&invalid).is_err());
    for source in [
        "export const run:number = 1;",
        "export const run=()=>import('./x');",
        "export const run=()=>require('./x');",
        "import x from './x'; export const run=()=>x;",
        "export const run=()=>import.meta;",
    ] {
        assert!(validate(&bundle(source)).is_err(), "{source}");
    }
    assert!(
        serde_json::from_str::<PreparedBundle>(
            r#"{"format_version":1,"modules":{},"functions":{},"unexpected":true}"#
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn artifact_export_star_cycle_and_json_size_limits_are_checked() -> TestResult {
    let mut prepared = bundle("export * from './second';");
    prepared
        .modules
        .get_mut("m_1")
        .ok_or("模块缺失")?
        .imports
        .insert("./second".into(), "m_2".into());
    prepared.modules.insert(
        "m_2".into(),
        PreparedModule {
            code: "export * from './first'; export function run(){return 1}".into(),
            imports: [("./first".into(), "m_1".into())].into(),
        },
    );
    validate(&prepared)?;
    let large = bundle(&format!(
        "/*{}*/export const run=()=>1",
        "x".repeat(MAX_MODULE_BYTES)
    ));
    assert!(validate(&large).is_err());
    Ok(())
}
