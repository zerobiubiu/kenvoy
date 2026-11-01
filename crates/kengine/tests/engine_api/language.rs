use super::*;

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
