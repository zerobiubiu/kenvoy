//! 正式 KEngine API 的真实语言、结构、文件系统和诊断测试。

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use kengine::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Project {
    path: PathBuf,
}

impl Project {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("kengine-api-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn write(&self, path: &str, source: &str) -> TestResult<PathBuf> {
        let path = self.path.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, source)?;
        Ok(path)
    }

    fn analyze(&self, entry: &str) -> TestResult<ProgramIR> {
        Ok(Engine::new().analyze_project(
            &self.path,
            AnalyzeOptions {
                entry_points: vec![entry.into()],
                ..Default::default()
            },
        )?)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // 仅清理本测试创建的临时目录，不操作项目工作区或外部模块目录。
        if self.path.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn parse(path: &str, source: &str) -> TestResult<ModuleIR> {
    let output = Engine::new().parse_source(Path::new(path), source, ParseOptions::default())?;
    assert!(!output.has_errors(), "实际诊断：{:?}", output.diagnostics);
    output.module.ok_or_else(|| "应返回已解析模块".into())
}

fn element<'a>(module: &'a ModuleIR, name: &str) -> TestResult<&'a ElementIR> {
    module
        .nodes
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Element(value) if value.name.text() == name => Some(value.as_ref()),
            _ => None,
        })
        .ok_or_else(|| format!("缺少元素 {name}").into())
}

#[test]
fn parses_real_javascript_typescript_jsx_and_tsx() -> TestResult {
    for (path, source, expected) in [
        (
            "a.js",
            "export const sum = (a, b) => a + b;",
            DeclarationKind::Const,
        ),
        (
            "a.ts",
            "export function identity<T>(input: T): T { return input; }",
            DeclarationKind::Function,
        ),
        (
            "a.jsx",
            "export const UI = () => <Widget enabled />;",
            DeclarationKind::Const,
        ),
        (
            "a.tsx",
            "export function UI<T>({value}:{value:T}) { return <Card value={value} />; }",
            DeclarationKind::Function,
        ),
    ] {
        let module = parse(path, source)?;
        assert_eq!(module.format_version, GENERIC_IR_VERSION);
        assert_eq!(module.declarations[0].kind, expected);
        assert_eq!(module.source, source);
        assert!(module.declarations[0].exported);
    }
    Ok(())
}

#[test]
fn finds_function_method_arrow_and_expression_jsx_without_execution() -> TestResult {
    let source = r#"
      function Component<T>(value: T) { return <FunctionView>{value}</FunctionView>; }
      class Example { render() { return <MethodView />; } }
      const Arrow = () => <ArrowView />;
      export default (<Root render={() => <RenderView />} icon={<Icon />}>
        {items.map(item => <Mapped value={item} />)}
      </Root>) satisfies unknown;
    "#;
    let module = parse("components.tsx", source)?;
    for name in [
        "FunctionView",
        "MethodView",
        "ArrowView",
        "Root",
        "RenderView",
        "Icon",
        "Mapped",
    ] {
        element(&module, name)?;
    }
    let default = module
        .exports
        .iter()
        .find_map(|export| match export.kind {
            ExportKind::DefaultExpression { expression } => module.expression(expression),
            _ => None,
        })
        .ok_or("缺少默认导出表达式")?;
    assert!(
        module
            .raw(&default.span)
            .ok_or("缺少 raw")?
            .contains("satisfies unknown")
    );
    assert_eq!(default.jsx_roots.len(), 1);
    let root = element(&module, "Root")?;
    let AttributeKind::Named {
        value:
            Some(AttributeValueIR::Expression {
                expression: Some(id),
                ..
            }),
        ..
    } = &root.attributes[0].kind
    else {
        return Err("应保留箭头函数表达式".into());
    };
    assert_eq!(
        module.expression(*id).ok_or("缺少表达式")?.kind,
        "ArrowFunction"
    );
    assert_eq!(
        module.expression(*id).ok_or("缺少表达式")?.jsx_roots.len(),
        1
    );
    // items 未声明不妨碍语法解析；阶段 01 不冒充完整符号检查或求值器。
    assert!(
        module
            .expressions
            .iter()
            .any(|expression| module.raw(&expression.span) == Some("items"))
    );
    Ok(())
}

#[test]
fn preserves_order_boolean_syntax_fragments_member_names_and_spread() -> TestResult {
    let module = parse(
        "structure.tsx",
        r#"export default <><UI.Card xml:lang="zh" enabled explicit={true} count={3} {...props} icon={<Icon />}>
      中文&#x1F600;{/* 注释 */}{value}{...items}<ns:leaf />
    </UI.Card></>;"#,
    )?;
    let card = element(&module, "UI.Card")?;
    assert_eq!(card.attributes.len(), 6);
    assert!(matches!(
        card.attributes[1].kind,
        AttributeKind::Named { value: None, .. }
    ));
    let AttributeKind::Named {
        value:
            Some(AttributeValueIR::Expression {
                expression: Some(explicit),
                span,
            }),
        ..
    } = &card.attributes[2].kind
    else {
        return Err("显式布尔值必须保留表达式容器".into());
    };
    assert_eq!(module.raw(span), Some("{true}"));
    assert_eq!(
        module.expression(*explicit).ok_or("缺少布尔值")?.literal,
        Some(LiteralIR::Boolean(true))
    );
    assert!(matches!(
        card.attributes[4].kind,
        AttributeKind::Spread { .. }
    ));
    assert!(
        module
            .nodes
            .iter()
            .any(|node| matches!(&node.kind, NodeKind::Fragment { .. }))
    );
    assert!(module.nodes.iter().any(|node| matches!(&node.kind, NodeKind::Text { value_utf16 } if String::from_utf16(value_utf16).is_ok_and(|text| text.contains("中文😀")))));
    assert!(
        module
            .nodes
            .iter()
            .any(|node| matches!(&node.kind, NodeKind::Expression { expression: None }))
    );
    assert!(
        module
            .nodes
            .iter()
            .any(|node| matches!(&node.kind, NodeKind::Spread { .. }))
    );
    assert_eq!(element(&module, "ns:leaf")?.name.text(), "ns:leaf");
    for node in &module.nodes {
        assert!(module.raw(&node.span).is_some());
    }
    for attribute in &card.attributes {
        assert!(module.raw(&attribute.span).is_some());
    }
    Ok(())
}

#[test]
fn strings_keep_lone_surrogates_and_numeric_raw_round_trip() -> TestResult {
    let module = parse(
        "literals.tsx",
        r#"export default <data value={"\ud800"} huge={1e999} large={12345678901234567890n} escaped="A&amp;B" />;"#,
    )?;
    let string = module
        .expressions
        .iter()
        .find_map(|expression| match &expression.literal {
            Some(LiteralIR::String(value)) => Some(value),
            _ => None,
        })
        .ok_or("缺少字符串字面量")?;
    assert_eq!(string.value_utf16, vec![0xd800]);
    assert!(string.to_string().is_err());
    assert_eq!(module.raw(&string.span), Some(r#""\ud800""#));
    assert!(module.expressions.iter().any(|expression| matches!(
        expression.literal,
        Some(LiteralIR::Number { value: None })
    ) && module.raw(&expression.span)
        == Some("1e999")));
    let serialized = serde_json::to_string(&module)?;
    let restored: ModuleIR = serde_json::from_str(&serialized)?;
    assert_eq!(module, restored);
    assert_eq!(restored.raw(&string.span), Some(r#""\ud800""#));
    Ok(())
}

#[test]
fn every_position_uses_original_bytes_and_unicode_scalar_columns() -> TestResult {
    let source = "\u{feff}// 中文😀\r\nexport default <root>中文😀<Child /></root>;\r\n";
    let module = parse("位置.tsx", source)?;
    assert_eq!(module.source, source);
    let child = module
        .nodes
        .iter()
        .find(|node| matches!(&node.kind, NodeKind::Element(value) if value.name.text() == "Child"))
        .ok_or("缺少 Child")?;
    assert_eq!(module.raw(&child.span), Some("<Child />"));
    assert_eq!(child.span.start_position.line, 2);
    let line_start = source.find("export").ok_or("缺少基准")?;
    assert_eq!(
        child.span.start_position.column,
        source[line_start..child.span.start].chars().count() + 1
    );
    assert_eq!(
        child.span.end_position.column - child.span.start_position.column,
        "<Child />".chars().count()
    );
    let separator = parse("separator.tsx", "//说明\u{2028}export default <Tag />;")?;
    let node = separator.node(separator.jsx_roots[0]).ok_or("缺少 Tag")?;
    assert_eq!(node.span.start_position.line, 2);
    Ok(())
}

#[test]
fn imports_exports_keep_aliases_types_namespaces_and_source_spans() -> TestResult {
    let source = r#"
      import Default, { type Shape as LocalShape, value as local } from './dep';
      import type TypeDefault from './types';
      import * as ns from './dep';
      import './side';
      export type { LocalShape as Renamed } from './types';
      export { local as value };
      export * as namespace from './dep';
      export * from './all';
      export default function Named<T>(value:T) { return value; }
    "#;
    let module = parse("module.ts", source)?;
    assert_eq!(module.imports.len(), 4);
    let first = &module.imports[0];
    assert_eq!(
        first.source.as_ref().ok_or("缺少来源")?.to_string()?,
        "./dep"
    );
    assert_eq!(first.specifiers[0].kind, ImportSpecifierKind::Default);
    assert_eq!(first.specifiers[1].local.name, "LocalShape");
    assert!(first.specifiers[1].type_only);
    assert!(!first.specifiers[2].type_only);
    assert!(
        matches!(&first.specifiers[2].imported, Some(ModuleNameIR::Identifier(value)) if value.name == "value")
    );
    assert!(module.imports[1].type_only);
    assert_eq!(
        module.imports[2].specifiers[0].kind,
        ImportSpecifierKind::Namespace
    );
    assert!(module.imports[3].specifiers.is_empty());
    assert!(module.exports[0].type_only);
    assert!(
        matches!(&module.exports[0].specifiers[0].exported, ModuleNameIR::Identifier(value) if value.name == "Renamed")
    );
    assert_eq!(
        module.exports[2].specifiers[0].kind,
        ExportSpecifierKind::Namespace
    );
    assert_eq!(module.exports[3].kind, ExportKind::All);
    assert!(
        matches!(&module.exports[4].kind, ExportKind::DefaultDeclaration { name: Some(name), .. } if name.name == "Named")
    );
    for import in &module.imports {
        assert!(
            module
                .raw(&import.span)
                .is_some_and(|raw| raw.trim_start().starts_with("import"))
        );
        for specifier in &import.specifiers {
            assert!(module.raw(&specifier.span).is_some());
        }
    }
    Ok(())
}

#[test]
fn declarations_include_destructuring_interfaces_classes_and_inner_variables() -> TestResult {
    let module = parse(
        "declarations.ts",
        r#"
      export const { original: renamed, shorthand = 1, ...rest } = input;
      export let [head, ...tail] = values;
      export interface Shape { value: number }
      export type Alias<T> = T;
      export enum Choice { First }
      export namespace Space { export const member = 1; }
      export class Example { method() { const inner = 1; return inner; } }
      const A = 1; export { A };
    "#,
    )?;
    let names: Vec<_> = module
        .declarations
        .iter()
        .flat_map(|declaration| {
            declaration
                .bindings
                .iter()
                .map(|binding| binding.name.as_str())
        })
        .collect();
    for name in [
        "renamed",
        "shorthand",
        "rest",
        "head",
        "tail",
        "Shape",
        "Alias",
        "Choice",
        "Space",
        "Example",
        "inner",
    ] {
        assert!(names.contains(&name), "缺少 {name}");
    }
    assert!(!names.contains(&"original"));
    assert!(
        module
            .declarations
            .iter()
            .any(|declaration| declaration.kind == DeclarationKind::Class && declaration.exported)
    );
    assert!(module.declarations.iter().any(|declaration| {
        declaration
            .bindings
            .iter()
            .any(|binding| binding.name == "inner")
            && !declaration.exported
    }));
    for name in ["A", "member"] {
        let declaration = module
            .declarations
            .iter()
            .find(|declaration| {
                declaration
                    .bindings
                    .iter()
                    .any(|binding| binding.name == name)
            })
            .ok_or("缺少非顶层内联导出标记的声明")?;
        assert!(
            !declaration.exported,
            "exported 仅标记顶层内联声明导出：{name}"
        );
    }
    Ok(())
}

#[test]
fn typescript_import_equals_and_export_assignment_are_retained() -> TestResult {
    let module = parse(
        "legacy.ts",
        "import Legacy = require('./legacy'); export = Legacy;",
    )?;
    assert_eq!(
        module.imports[0].specifiers[0].kind,
        ImportSpecifierKind::TypeScriptEquals
    );
    assert_eq!(
        module.imports[0]
            .source
            .as_ref()
            .ok_or("缺少来源")?
            .to_string()?,
        "./legacy"
    );
    assert!(matches!(
        module.exports[0].kind,
        ExportKind::TypeScriptAssignment { .. }
    ));
    let namespace = parse("ambient.ts", "export as namespace Library;")?;
    assert!(
        matches!(&namespace.exports[0].kind, ExportKind::TypeScriptNamespace { name } if name.name == "Library")
    );
    Ok(())
}

#[test]
fn fatal_and_recovered_errors_are_structured_and_never_reported_as_valid() -> TestResult {
    let engine = Engine::new();
    for (name, source) in [
        (
            "fatal.tsx",
            "const label = '中文😀'; export default <Broken>",
        ),
        (
            "recovered.ts",
            "const value = 1; const enum D { a = value b = value }",
        ),
    ] {
        let parsed = engine.parse_source(Path::new(name), source, ParseOptions::default())?;
        assert!(parsed.has_errors());
        for diagnostic in parsed.get_diagnostics() {
            assert_eq!(diagnostic.code, "KE1001");
            assert_eq!(diagnostic.category, DiagnosticCategory::Syntax);
            assert_eq!(diagnostic.path, Path::new(name));
            assert!(!diagnostic.message.is_empty());
            assert!(!diagnostic.suggestion.is_empty());
            let span = diagnostic.span.as_ref().ok_or("缺少位置")?;
            assert!(span.raw(source).is_some());
            assert_eq!(
                span.start_position.column,
                source[..span.start].chars().count() + 1
            );
        }
    }
    Ok(())
}

#[test]
fn resolves_relative_index_dotted_names_js_mapping_and_canonical_aliases() -> TestResult {
    let project = Project::new()?;
    let entry = project.write("src/main.ts", "export {};")?;
    let dotted = project.write("src/Fields.diu.tsx", "export default <field />;")?;
    let index = project.write(
        "src/components/index.tsx",
        "export const Component = () => <div />;",
    )?;
    let mapped = project.write("src/math.ts", "export const amount = 1;")?;
    let options = ResolveOptions {
        root: project.path.clone(),
        ..Default::default()
    };
    for (specifier, target) in [
        ("./Fields.diu", dotted),
        ("./components", index),
        ("./math.js", mapped.clone()),
        ("./components/../math", mapped),
    ] {
        assert_eq!(
            Engine::new().resolve_module(&entry, specifier, &options)?,
            ResolvedModule::File {
                path: target.canonicalize()?
            }
        );
    }
    let exact = project.write("src/math.js", "export const amount = 2;")?;
    assert_eq!(
        Engine::new().resolve_module(&entry, "./math.js", &options)?,
        ResolvedModule::File {
            path: exact.canonicalize()?
        }
    );
    Ok(())
}

#[test]
fn module_graph_traverses_reexports_type_edges_and_records_actual_cycles() -> TestResult {
    let project = Project::new()?;
    project.write("main.tsx", "import { B } from './nested'; export { value } from './alias.diu'; export type { Kind } from './types'; export default <Unknown />;")?;
    let nested = project.write(
        "nested/index.tsx",
        "import { value } from '../alias.diu'; export const B = () => <Other />;",
    )?;
    let alias = project.write(
        "alias.diu.ts",
        "export { B } from './nested'; export const value = 1;",
    )?;
    project.write("types.ts", "export type Kind = string;")?;
    let program = project.analyze("main.tsx")?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    assert_eq!(program.modules.len(), 4);
    assert_eq!(program.get_module_graph().nodes.len(), 4);
    assert_eq!(program.module_graph.edges.len(), 5);
    assert!(
        program
            .module_graph
            .edges
            .iter()
            .any(|edge| edge.kind == DependencyKind::TypeReexport)
    );
    let mut expected = vec![nested.canonicalize()?, alias.canonicalize()?];
    expected.sort();
    assert_eq!(program.module_graph.cycles, vec![expected]);
    assert_eq!(
        program,
        serde_json::from_str::<ProgramIR>(&serde_json::to_string(&program)?)?
    );
    Ok(())
}

#[test]
fn duplicate_paths_and_self_cycles_terminate_with_one_module() -> TestResult {
    let project = Project::new()?;
    project.write(
        "self.ts",
        "import './self'; import './directory/../self'; export const value = 1;",
    )?;
    fs::create_dir(project.path.join("directory"))?;
    let program = project.analyze("self.ts")?;
    assert!(!program.has_errors());
    assert_eq!(program.modules.len(), 1);
    assert_eq!(program.module_graph.edges.len(), 2);
    assert_eq!(
        program.module_graph.cycles,
        vec![vec![project.path.join("self.ts").canonicalize()?]]
    );
    Ok(())
}

#[test]
fn bare_imports_require_explicit_exact_externals_and_keep_unresolved_edges() -> TestResult {
    let project = Project::new()?;
    project.write("main.ts", "import UI from 'some-ui'; import type { Type } from './types'; export const later = () => import('./dynamic');")?;
    project.write("types.ts", "export type Type = string;")?;
    let invalid = project.analyze("main.ts")?;
    assert!(invalid.has_errors());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KE2003")
    );
    assert!(
        invalid
            .module_graph
            .edges
            .iter()
            .any(|edge| edge.specifier == "some-ui" && edge.target.is_none())
    );
    let valid = Engine::new().analyze_project(
        &project.path,
        AnalyzeOptions {
            entry_points: vec!["main.ts".into()],
            resolve: ResolveOptions {
                externals: ["some-ui".into()].into(),
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    assert!(!valid.has_errors(), "{:?}", valid.diagnostics);
    assert!(valid.module_graph.edges.iter().any(|edge| matches!(&edge.target, Some(ResolvedModule::External { specifier }) if specifier == "some-ui")));
    assert!(
        valid
            .module_graph
            .edges
            .iter()
            .any(|edge| edge.kind == DependencyKind::TypeImport)
    );
    assert_eq!(
        valid.module_graph.edges.len(),
        2,
        "动态 import 未冒充静态依赖"
    );
    Ok(())
}

#[test]
fn missing_modules_and_root_escapes_report_import_locations() -> TestResult {
    let project = Project::new()?;
    project.write("app/main.ts", "import '../outside'; import './missing';")?;
    project.write("outside.ts", "export const secret = 1;")?;
    let program = Engine::new().analyze_project(
        &project.path.join("app"),
        AnalyzeOptions {
            entry_points: vec!["main.ts".into()],
            ..Default::default()
        },
    )?;
    assert!(program.has_errors());
    assert_eq!(program.modules.len(), 1);
    for code in ["KE2001", "KE2002"] {
        let diagnostic = program
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .ok_or("缺少模块诊断")?;
        assert!(diagnostic.span.is_some());
        assert!(diagnostic.path.ends_with("main.ts"));
    }
    Ok(())
}

#[test]
fn independent_results_io_failures_and_explicit_virtual_language() -> TestResult {
    let engine = Engine::new();
    let first = engine.parse_source(
        Path::new("a.tsx"),
        "export default <first />;",
        ParseOptions::default(),
    )?;
    let second = engine.parse_source(
        Path::new("b.tsx"),
        "export default <second />;",
        ParseOptions::default(),
    )?;
    element(first.module.as_ref().ok_or("缺少 first")?, "first")?;
    element(second.module.as_ref().ok_or("缺少 second")?, "second")?;
    let virtual_file = engine.parse_source(
        Path::new("virtual"),
        "export const value: number = 1;",
        ParseOptions {
            language: Language::TypeScript,
        },
    )?;
    assert!(!virtual_file.has_errors());
    let unknown = engine.parse_source(Path::new("virtual"), "value", ParseOptions::default())?;
    assert_eq!(unknown.diagnostics[0].code, "KE0001");
    let project = Project::new()?;
    let missing = project.path.join("absent.ts");
    let error = engine
        .parse_file(&missing, ParseOptions::default())
        .err()
        .ok_or("必须返回 IO 错误")?;
    assert!(matches!(&error, EngineError::Io { path, .. } if path == &missing));
    assert!(error.source().is_some());
    let no_entries = engine.analyze_project(&project.path, AnalyzeOptions::default())?;
    assert_eq!(no_entries.diagnostics[0].code, "KE0002");
    Ok(())
}

#[test]
fn direct_identifier_text_does_not_claim_scope_or_business_semantics() -> TestResult {
    let module = parse(
        "generic.tsx",
        "const handler = 1; function nested(handler: number) { return <anything onClick={handler} />; } export default <page custom={handler} />;",
    )?;
    let references: Vec<_> = module
        .expressions
        .iter()
        .filter_map(|expression| expression.direct_identifier.as_ref())
        .filter(|identifier| identifier.name == "handler")
        .collect();
    assert_eq!(references.len(), 2);
    assert_ne!(references[0].span, references[1].span);
    assert_eq!(element(&module, "page")?.name.text(), "page");
    assert!(module.imports.is_empty());
    // 未出现页面、路由、数据库字段或权限语义，任意标签都作为通用 JSX 保留。
    Ok(())
}

#[test]
fn unknown_ir_versions_are_rejected_including_nested_modules() -> TestResult {
    let module = parse("version.ts", "export const value = 1;")?;
    let mut encoded = serde_json::to_value(&module)?;
    encoded["format_version"] = 99.into();
    let error = serde_json::from_value::<ModuleIR>(encoded)
        .err()
        .ok_or("不能接受未知模块 IR 版本")?;
    assert!(error.to_string().contains("不支持 Generic IR 格式版本 99"));
    let project = Project::new()?;
    project.write("main.ts", "export const value = 1;")?;
    let program = project.analyze("main.ts")?;
    let mut encoded = serde_json::to_value(&program)?;
    encoded["format_version"] = 99.into();
    assert!(serde_json::from_value::<ProgramIR>(encoded).is_err());
    let mut encoded = serde_json::to_value(&program)?;
    encoded["modules"][0]["format_version"] = 99.into();
    assert!(serde_json::from_value::<ProgramIR>(encoded).is_err());
    Ok(())
}

#[test]
fn invalid_utf8_and_invalid_utf16_module_paths_are_explicit_errors() -> TestResult {
    let project = Project::new()?;
    let invalid = project.path.join("invalid.ts");
    fs::write(&invalid, [0xff, 0xfe])?;
    assert!(matches!(
        Engine::new().parse_file(&invalid, ParseOptions::default()),
        Err(EngineError::Io { .. })
    ));
    project.write("main.ts", r#"import value from "\ud800";"#)?;
    let result = project.analyze("main.ts")?;
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KE2003" && diagnostic.span.is_some())
    );
    assert_eq!(
        result.modules[0].imports[0]
            .source
            .as_ref()
            .ok_or("缺少模块说明符")?
            .value_utf16,
        vec![0xd800]
    );
    assert!(
        result.module_graph.edges.is_empty(),
        "无法无损转换为 String 的说明符保留 ImportIR 与诊断，不生成模块边"
    );
    Ok(())
}
