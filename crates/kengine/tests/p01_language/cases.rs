use super::*;

#[test]
fn parses_four_real_language_modes() -> TestResult {
    let cases = [
        (
            "a.js",
            "export const value = (x) => x + 1;",
            Syntax::Es(EsSyntax::default()),
        ),
        (
            "a.ts",
            "export function id<T>(x:T):T { return x; }",
            Syntax::Typescript(TsSyntax::default()),
        ),
        (
            "a.jsx",
            "export default <Widget enabled />;",
            Syntax::Es(EsSyntax {
                jsx: true,
                ..Default::default()
            }),
        ),
        (
            "a.tsx",
            "const id = <T,>(x:T):T => x; export default <Widget value={id(3)} />;",
            tsx(),
        ),
    ];
    for (path, source, syntax) in cases {
        Parsed::parse(path, source, syntax)?;
    }
    Ok(())
}

#[test]
fn preserves_fragments_attributes_order_literals_and_function_reference() -> TestResult {
    let parsed = Parsed::parse("main.tsx", include_str!("../fixtures/p01/main.tsx"), tsx())?;
    let module = parsed.convert("main.tsx");
    let Some(Node::Fragment(children, _)) = &module.root else {
        panic!("应保留 Fragment");
    };
    let panel = children
        .iter()
        .find(|node| matches!(node, Node::Element { name, .. } if name == "panel"));
    let Some(Node::Element {
        attributes,
        children,
        ..
    }) = panel
    else {
        panic!("应保留普通 JSX 元素");
    };
    assert!(
        matches!(&attributes[0], Attribute::Named { name, value: Value::String(value) } if name == "title" && value == "客户😀")
    );
    assert!(matches!(
        &attributes[1],
        Attribute::Named {
            value: Value::Boolean(true),
            ..
        }
    ));
    assert!(
        matches!(&attributes[2], Attribute::Named { value: Value::Number(value), .. } if *value == 3.0)
    );
    assert!(matches!(
        &attributes[3],
        Attribute::Named {
            value: Value::Boolean(false),
            ..
        }
    ));
    assert!(
        matches!(&attributes[4], Attribute::Spread(value) if value.symbol.as_deref() == Some("attributes"))
    );
    assert!(
        matches!(&attributes[5], Attribute::Named { value: Value::Expression(value), .. } if value.symbol.as_deref() == Some("discount"))
    );
    assert!(
        children
            .iter()
            .any(|node| matches!(node, Node::Element { name, .. } if name == "Card"))
    );
    assert!(children.iter().any(|node| matches!(node, Node::Expression(value) if value.kind == "Conditional" && value.source.contains("amount > 0"))));
    let serialized = serde_json::to_string(&module)?;
    assert_eq!(module, serde_json::from_str::<ProbeModule>(&serialized)?);
    Ok(())
}

#[test]
fn associates_imports_exports_and_real_local_modules() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/p01");
    let graph = analyze_fixture(&root, &root.join("main.tsx"))?;
    assert_eq!(graph.len(), 3);
    let entry = &graph[&root.join("main.tsx")];
    assert!(
        entry
            .imports
            .iter()
            .any(|import| import.type_only && import.bindings == ["Amount"])
    );
    for export in ["amount", "identity", "calculateDiscount", "default"] {
        assert!(entry.exports.iter().any(|name| name == export));
    }
    assert!(
        entry
            .imports
            .iter()
            .any(|import| import.bindings == ["Card"] && import.source == "./ui")
    );
    Ok(())
}

#[test]
fn retains_member_names_nested_attribute_nodes_and_unknown_expressions() -> TestResult {
    let parsed = Parsed::parse(
        "names.tsx",
        "export default <UI.Card xml:lang=\"zh\" icon={<Icon />} value={values.map(x => x)}>{...items}</UI.Card>;",
        tsx(),
    )?;
    let Some(Node::Element {
        name,
        attributes,
        children,
        ..
    }) = parsed.convert("names.tsx").root
    else {
        panic!("应保留 Element");
    };
    assert_eq!(name, "UI.Card");
    assert!(matches!(&attributes[0], Attribute::Named { name, .. } if name == "xml:lang"));
    assert!(matches!(
        &attributes[1],
        Attribute::Named {
            value: Value::Node(_),
            ..
        }
    ));
    assert!(
        matches!(&attributes[2], Attribute::Named { value: Value::Expression(value), .. } if value.source == "values.map(x => x)")
    );
    assert!(
        matches!(&children[0], Node::Spread(value) if value.symbol.as_deref() == Some("items"))
    );
    Ok(())
}

#[test]
fn tracks_unicode_columns_crlf_and_half_open_byte_ranges() -> TestResult {
    let source = "// 中文😀\r\nexport default <panel>中文😀<Widget /></panel>;\r\n";
    let parsed = Parsed::parse("位置.tsx", source, tsx())?;
    let Some(Node::Element { children, .. }) = parsed.convert("位置.tsx").root else {
        panic!("应保留根节点");
    };
    let Node::Element { location, .. } = &children[1] else {
        panic!("应保留组件位置");
    };
    assert_eq!(&source[location.start..location.end], "<Widget />");
    assert_eq!(location.line, 2);
    let line_start = source.find("export").ok_or("缺少位置基准")?;
    assert_eq!(
        location.column,
        source[line_start..location.start].chars().count() + 1
    );
    Ok(())
}

#[test]
fn rejects_fatal_and_recovered_syntax_errors_with_context() -> TestResult {
    let error = Parsed::parse("broken.tsx", "export default <Widget>;", tsx());
    assert!(
        matches!(error, Err(ProbeError::Syntax { ref path, line: 1, .. }) if path == "broken.tsx")
    );
    let source_map: Lrc<SourceMap> = Default::default();
    let source = "const value = 1; const enum D { a = value b = value }";
    let file = source_map.new_source_file(FileName::Custom("recover.ts".into()).into(), source);
    let mut errors = Vec::new();
    let result = parse_file_as_module(
        &file,
        Syntax::Typescript(TsSyntax::default()),
        EsVersion::EsNext,
        None,
        &mut errors,
    );
    assert!(result.is_ok(), "此夹具应由 SWC 恢复产生 AST");
    assert!(!errors.is_empty(), "成功 AST 不能掩盖恢复错误");
    assert!(
        Parsed::parse(
            "recover.ts",
            source,
            Syntax::Typescript(TsSyntax::default())
        )
        .is_err()
    );
    Ok(())
}
