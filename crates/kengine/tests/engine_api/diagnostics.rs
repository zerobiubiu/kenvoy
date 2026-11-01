use super::*;

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
