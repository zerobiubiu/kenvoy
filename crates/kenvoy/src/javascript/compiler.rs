//! 服务端模块的 TypeScript 转换与可持久化白名单；不在编译阶段执行项目代码。

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

use kengine::{ProgramIR, ResolvedModule};
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::{self as ast, EsVersion};
use swc_ecma_codegen::to_code_default;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};
use swc_ecma_transforms_base::{fixer::fixer, resolver};
use swc_ecma_transforms_typescript::strip;
use swc_ecma_visit::{Visit, VisitWith};

use super::{BUNDLE_VERSION, FunctionTarget, JsDiagnostic, PreparedBundle, PreparedModule, Trust};
use crate::{
    application::{ApplicationIR, ModuleReferenceKind},
    dsl::SourceLocation,
};

const MAX_MODULES: usize = 256;
const MAX_FUNCTIONS: usize = 1024;
const MAX_MODULE_BYTES: usize = 512 * 1024;
const MAX_BUNDLE_BYTES: usize = 4 * 1024 * 1024;

fn failure(message: impl Into<String>) -> JsDiagnostic {
    JsDiagnostic::new(
        "KV6001",
        message,
        None,
        "使用可信项目中的本地静态 ES Module，检查登记的函数与依赖",
    )
}

fn location(name: &str, source: &str, start: usize, end: usize) -> SourceLocation {
    let position = |offset: usize| {
        let prefix = &source[..offset.min(source.len())];
        let mut line = 1;
        let mut column = 1;
        let mut last_cr = false;
        for ch in prefix.chars() {
            match ch {
                '\n' if last_cr => {
                    last_cr = false;
                }
                '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                    line += 1;
                    column = 1;
                    last_cr = ch == '\r';
                }
                _ => {
                    column += 1;
                    last_cr = false;
                }
            }
        }
        kengine::Position { line, column }
    };
    SourceLocation {
        path: PathBuf::from(name),
        span: kengine::SourceSpan {
            start,
            end,
            start_position: position(start),
            end_position: position(end),
        },
    }
}

fn parse(
    source: &str,
    name: &str,
    typescript: bool,
    jsx: bool,
) -> Result<(Lrc<SourceMap>, ast::Module, u32), JsDiagnostic> {
    if source.len() > MAX_MODULE_BYTES {
        return Err(failure("单个 JavaScript 模块超过 512 KiB 限额"));
    }
    let map: Lrc<SourceMap> = Default::default();
    let file = map.new_source_file(FileName::Custom(name.into()).into(), source.to_owned());
    let base = file.start_pos.0;
    let syntax = if typescript {
        Syntax::Typescript(TsSyntax {
            tsx: jsx,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax {
            jsx,
            ..Default::default()
        })
    };
    let mut recovered = Vec::new();
    let result = parse_file_as_module(&file, syntax, EsVersion::latest(), None, &mut recovered);
    let error = match result {
        Ok(module) if recovered.is_empty() => return Ok((map, module, base)),
        Ok(_) => recovered.remove(0),
        Err(error) => error,
    };
    let span = error.span();
    Err(JsDiagnostic::new(
        "KV6002",
        format!("JavaScript 模块语法无效：{:?}", error.kind()),
        Some(location(
            name,
            source,
            span.lo.0.saturating_sub(base) as usize,
            span.hi.0.saturating_sub(base) as usize,
        )),
        "修正模块语法后重新构建",
    ))
}

/// 客户端源码切片复用此转换器；JSX 保留给前端构建工具，不生成浏览器运行代码。
pub(super) fn transpile(source: &str, name: &str, jsx: bool) -> Result<String, JsDiagnostic> {
    let (map, module, _) = parse(source, name, true, jsx)?;
    Ok(GLOBALS.set(&Globals::default(), || {
        let unresolved = Mark::new();
        let top_level = Mark::new();
        let program = ast::Program::Module(module)
            .apply(resolver(unresolved, top_level, true))
            .apply(strip(unresolved, top_level))
            .apply(fixer(None));
        to_code_default(map, None, &program)
    }))
}

#[derive(Default)]
struct Restrictions {
    denied: Option<(Span, &'static str)>,
}
impl Restrictions {
    fn deny(&mut self, span: Span, reason: &'static str) {
        if self.denied.is_none() {
            self.denied = Some((span, reason));
        }
    }
}
impl Visit for Restrictions {
    fn visit_jsx_element(&mut self, node: &ast::JSXElement) {
        self.deny(node.span, "服务端计算模块不支持 JSX");
    }
    fn visit_jsx_fragment(&mut self, node: &ast::JSXFragment) {
        self.deny(node.span, "服务端计算模块不支持 JSX Fragment");
    }
    fn visit_call_expr(&mut self, node: &ast::CallExpr) {
        if matches!(node.callee, ast::Callee::Import(_)) {
            self.deny(node.span, "服务端模块不支持动态 import");
        }
        node.visit_children_with(self);
    }
    fn visit_meta_prop_expr(&mut self, node: &ast::MetaPropExpr) {
        if node.kind == ast::MetaPropKind::ImportMeta {
            self.deny(node.span, "服务端模块不开放 import.meta");
        }
    }
    fn visit_ident(&mut self, node: &ast::Ident) {
        if matches!(node.sym.as_ref(), "require" | "eval" | "Function") {
            self.deny(node.span, "服务端模块不支持 require 或动态代码构造");
        }
    }
    fn visit_import_decl(&mut self, node: &ast::ImportDecl) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 import attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_named_export(&mut self, node: &ast::NamedExport) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 export attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_export_all(&mut self, node: &ast::ExportAll) {
        if node.with.is_some() {
            self.deny(node.span, "服务端模块不支持 export attributes");
        }
        node.visit_children_with(self);
    }
    fn visit_ts_import_equals_decl(&mut self, node: &ast::TsImportEqualsDecl) {
        self.deny(node.span, "服务端模块不支持 TypeScript import =");
    }
    fn visit_ts_export_assignment(&mut self, node: &ast::TsExportAssignment) {
        self.deny(node.span, "服务端模块不支持 TypeScript export =");
    }
}

fn restricted_parse(
    source: &str,
    name: &str,
    typescript: bool,
) -> Result<ast::Module, JsDiagnostic> {
    let jsx = typescript && (name.ends_with(".tsx") || name.ends_with(".jsx"));
    let (_, module, base) = parse(source, name, typescript, jsx)?;
    let mut restrictions = Restrictions::default();
    module.visit_with(&mut restrictions);
    if let Some((span, reason)) = restrictions.denied {
        return Err(JsDiagnostic::new(
            "KV6003",
            reason,
            Some(location(
                name,
                source,
                span.lo.0.saturating_sub(base) as usize,
                span.hi.0.saturating_sub(base) as usize,
            )),
            "将模块改为本地静态 ESM 纯计算函数；浏览器组件必须放入客户端模块",
        ));
    }
    Ok(module)
}

fn text(value: &ast::Str) -> Result<String, JsDiagnostic> {
    value
        .value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| failure("模块名称包含无法无损转换的 UTF-16 字符"))
}

fn imports(module: &ast::Module) -> Result<BTreeSet<String>, JsDiagnostic> {
    let mut result = BTreeSet::new();
    for item in &module.body {
        let ast::ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        let source = match decl {
            ast::ModuleDecl::Import(import) => Some(&import.src),
            ast::ModuleDecl::ExportNamed(export) => export.src.as_ref(),
            ast::ModuleDecl::ExportAll(export) => Some(&export.src),
            _ => None,
        };
        if let Some(source) = source {
            result.insert(text(source)?);
        }
    }
    Ok(result)
}

fn valid_specifier(value: &str) -> bool {
    (value.starts_with("./") || value.starts_with("../"))
        && value.len() <= 512
        && !value.contains(['\0', '\\', ':', '?', '#'])
}

/// 只消费 KEngine 公共 IR；可达模块集合与路径均来自已经解析的模块图。
pub fn prepare(
    program: &ProgramIR,
    application: &ApplicationIR,
    trust: Trust,
) -> Result<PreparedBundle, JsDiagnostic> {
    if trust != Trust::Project {
        return Err(failure("尚未提供操作系统隔离，拒绝执行不可信 JavaScript"));
    }
    if program.has_errors() {
        return Err(failure("源码分析存在错误，不能准备服务端模块"));
    }
    let roots: Vec<_> = application
        .module_references
        .iter()
        .filter(|reference| {
            matches!(
                reference.kind,
                ModuleReferenceKind::ServerFunction | ModuleReferenceKind::SharedFunction
            )
        })
        .collect();
    if roots.len() > MAX_FUNCTIONS {
        return Err(failure("服务端函数登记超过 1024 项限额"));
    }
    let mut queue: VecDeque<_> = roots
        .iter()
        .map(|reference| reference.module.clone())
        .collect();
    let mut compiled = BTreeMap::new();
    let mut total = 0usize;
    while let Some(path) = queue.pop_front() {
        if compiled.contains_key(&path) {
            continue;
        }
        if compiled.len() >= MAX_MODULES {
            return Err(failure("服务端可达模块超过 256 项限额"));
        }
        let module = program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(|| failure("服务端模块必须属于已分析的本地源码集合"))?;
        let name = path.to_string_lossy();
        restricted_parse(&module.source, &name, true)?;
        let code = transpile(&module.source, &name, false)?;
        total = total
            .checked_add(code.len())
            .ok_or_else(|| failure("模块总大小溢出"))?;
        if total > MAX_BUNDLE_BYTES {
            return Err(failure("服务端模块总大小超过 4 MiB 限额"));
        }
        let compiled_ast = restricted_parse(&code, &name, false)?;
        let mut dependencies = BTreeMap::new();
        for specifier in imports(&compiled_ast)? {
            if !valid_specifier(&specifier) {
                return Err(failure(format!(
                    "服务端不支持包、URL 或绝对路径依赖 {specifier}"
                )));
            }
            let target = program
                .module_graph
                .edges
                .iter()
                .find(|edge| edge.importer == path && edge.specifier == specifier)
                .and_then(|edge| edge.target.as_ref());
            let Some(ResolvedModule::File { path: dependency }) = target else {
                return Err(failure(format!(
                    "服务端静态依赖 {specifier} 没有已验证的本地模块目标"
                )));
            };
            dependencies.insert(specifier, dependency.clone());
            queue.push_back(dependency.clone());
        }
        compiled.insert(path, (code, dependencies));
    }
    let ids: BTreeMap<_, _> = compiled
        .keys()
        .enumerate()
        .map(|(index, path)| (path.clone(), format!("m_{}", index + 1)))
        .collect();
    let mut bundle = PreparedBundle {
        format_version: BUNDLE_VERSION,
        modules: BTreeMap::new(),
        functions: BTreeMap::new(),
    };
    for (path, (code, dependencies)) in compiled {
        let imports = dependencies
            .into_iter()
            .map(|(specifier, target)| {
                ids.get(&target)
                    .cloned()
                    .map(|id| (specifier, id))
                    .ok_or_else(|| failure("服务端依赖图闭包不完整"))
            })
            .collect::<Result<_, _>>()?;
        bundle
            .modules
            .insert(ids[&path].clone(), PreparedModule { code, imports });
    }
    for reference in roots {
        if reference.external || reference.expression.is_some() {
            return Err(failure("服务端函数必须使用本地静态导出登记"));
        }
        let module = ids
            .get(&reference.module)
            .cloned()
            .ok_or_else(|| failure("登记的服务端模块未编译"))?;
        if bundle
            .functions
            .insert(
                reference.id.clone(),
                FunctionTarget {
                    module,
                    export: reference.export.clone(),
                },
            )
            .is_some()
        {
            return Err(failure("服务端函数登记 ID 重复"));
        }
    }
    validate(&bundle)?;
    Ok(bundle)
}

fn valid_module_id(id: &str) -> bool {
    id.strip_prefix("m_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 10
            && !suffix.starts_with('0')
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn export_name(name: &ast::ModuleExportName) -> Result<String, JsDiagnostic> {
    match name {
        ast::ModuleExportName::Ident(name) => Ok(name.sym.to_string()),
        ast::ModuleExportName::Str(name) => text(name),
    }
}

fn pattern_names(pattern: &ast::Pat, names: &mut BTreeSet<String>) {
    match pattern {
        ast::Pat::Ident(name) => {
            names.insert(name.id.sym.to_string());
        }
        ast::Pat::Array(array) => {
            for item in array.elems.iter().flatten() {
                pattern_names(item, names);
            }
        }
        ast::Pat::Object(object) => {
            for item in &object.props {
                match item {
                    ast::ObjectPatProp::KeyValue(item) => pattern_names(&item.value, names),
                    ast::ObjectPatProp::Assign(item) => {
                        names.insert(item.key.id.sym.to_string());
                    }
                    ast::ObjectPatProp::Rest(item) => pattern_names(&item.arg, names),
                }
            }
        }
        ast::Pat::Assign(item) => pattern_names(&item.left, names),
        ast::Pat::Rest(item) => pattern_names(&item.arg, names),
        _ => {}
    }
}

/// 构建产物不是可信输入；拒绝未知版本、路径替代模块 ID、漏项映射和动态加载能力。
pub fn validate(bundle: &PreparedBundle) -> Result<(), JsDiagnostic> {
    if bundle.format_version != BUNDLE_VERSION
        || bundle.modules.len() > MAX_MODULES
        || bundle.functions.len() > MAX_FUNCTIONS
    {
        return Err(failure("JavaScript 产物版本或登记数量不受支持"));
    }
    let mut total = 0usize;
    let mut exports = BTreeMap::<String, BTreeSet<String>>::new();
    let mut stars = Vec::new();
    for (id, module) in &bundle.modules {
        if !valid_module_id(id) {
            return Err(failure("JavaScript 产物包含非法模块 ID"));
        }
        total = total
            .checked_add(module.code.len())
            .ok_or_else(|| failure("模块总大小溢出"))?;
        if total > MAX_BUNDLE_BYTES {
            return Err(failure("JavaScript 产物超过 4 MiB 限额"));
        }
        let ast = restricted_parse(&module.code, id, false)?;
        let required = imports(&ast)?;
        if required != module.imports.keys().cloned().collect() {
            return Err(failure("模块 import 白名单与代码中的静态依赖不一致"));
        }
        for (specifier, target) in &module.imports {
            if !valid_specifier(specifier) || !bundle.modules.contains_key(target) {
                return Err(failure("模块依赖不在已登记的本地白名单中"));
            }
        }
        let mut names = BTreeSet::new();
        for item in ast.body {
            let ast::ModuleItem::ModuleDecl(decl) = item else {
                continue;
            };
            match decl {
                ast::ModuleDecl::ExportDefaultDecl(_) | ast::ModuleDecl::ExportDefaultExpr(_) => {
                    names.insert("default".into());
                }
                ast::ModuleDecl::ExportDecl(export) => match export.decl {
                    ast::Decl::Fn(item) => {
                        names.insert(item.ident.sym.to_string());
                    }
                    ast::Decl::Class(item) => {
                        names.insert(item.ident.sym.to_string());
                    }
                    ast::Decl::Var(item) => {
                        for declaration in item.decls {
                            pattern_names(&declaration.name, &mut names);
                        }
                    }
                    _ => {}
                },
                ast::ModuleDecl::ExportNamed(export) => {
                    for specifier in export.specifiers {
                        names.insert(match specifier {
                            ast::ExportSpecifier::Named(item) => {
                                export_name(item.exported.as_ref().unwrap_or(&item.orig))?
                            }
                            ast::ExportSpecifier::Namespace(item) => export_name(&item.name)?,
                            ast::ExportSpecifier::Default(item) => item.exported.sym.to_string(),
                        });
                    }
                }
                ast::ModuleDecl::ExportAll(export) => {
                    stars.push((id.clone(), module.imports[&text(&export.src)?].clone()));
                }
                _ => {}
            }
        }
        exports.insert(id.clone(), names);
    }
    for _ in 0..bundle.modules.len() {
        let mut changed = false;
        for (id, target) in &stars {
            let target_names: Vec<_> = exports[target]
                .iter()
                .filter(|name| name.as_str() != "default")
                .cloned()
                .collect();
            let names = exports
                .get_mut(id)
                .ok_or_else(|| failure("模块导出闭包缺失"))?;
            for name in target_names {
                changed |= names.insert(name);
            }
        }
        if !changed {
            break;
        }
    }
    for (id, target) in &bundle.functions {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || target.export.is_empty()
            || target.export.len() > 256
            || target.export.contains('\0')
            || !exports
                .get(&target.module)
                .is_some_and(|names| names.contains(&target.export))
        {
            return Err(failure("JavaScript 函数登记 ID、模块或导出不存在"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{APPLICATION_IR_VERSION, ModuleReference};
    use kengine::{AnalyzeOptions, Engine, ResolveOptions};
    use std::{
        error::Error,
        fs,
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
    fn ts_static_modules_reexports_async_and_type_only_imports_prepare_without_execution()
    -> TestResult {
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
        assert!(
            prepared
                .modules
                .values()
                .all(|module| !module.code.contains("interface")
                    && !module.code.contains("extends number"))
        );
        assert!(
            !serde_json::to_string(&prepared)?.contains(&project.0.to_string_lossy().to_string())
        );
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
        let output = transpile(
            "export function Card<T>({value}:{value:T}) { return <div>{String(value)}</div> }",
            "Card.tsx",
            true,
        )?;
        assert!(output.contains("<div>"));
        assert!(!output.contains("value:T") && !output.contains("<T>"));
        assert!(transpile("export const x: =", "invalid.ts", false).is_err());
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
}
