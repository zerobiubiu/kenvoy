//! 表达式与变量初始化结构的公开 API 回归；不执行用户源码。

use std::{error::Error, path::Path};

use kengine::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn parse(path: &str, source: &str) -> TestResult<ModuleIR> {
    let parsed = Engine::new().parse_source(Path::new(path), source, ParseOptions::default())?;
    assert!(!parsed.has_errors(), "实际诊断：{:?}", parsed.diagnostics);
    parsed.module.ok_or_else(|| "缺少模块".into())
}

fn default_expression(module: &ModuleIR) -> TestResult<ExpressionId> {
    module
        .exports
        .iter()
        .find_map(|export| match export.kind {
            ExportKind::DefaultExpression { expression } => Some(expression),
            _ => None,
        })
        .ok_or_else(|| "缺少默认表达式导出".into())
}

fn initializer(module: &ModuleIR, name: &str) -> TestResult<ExpressionId> {
    module
        .declarations
        .iter()
        .flat_map(|declaration| &declaration.declarators)
        .find_map(|declarator| match &declarator.pattern.kind {
            BindingPatternKind::Identifier(identifier) if identifier.name == name => {
                declarator.initializer
            }
            _ => None,
        })
        .ok_or_else(|| format!("缺少 {name} 的初始值").into())
}

fn expression(module: &ModuleIR, id: ExpressionId) -> TestResult<&ExpressionIR> {
    module
        .expression(id)
        .ok_or_else(|| "表达式引用超出范围".into())
}

#[test]
fn default_export_structure_identifies_its_root_without_executing_function_jsx() -> TestResult {
    let module = parse(
        "entry.tsx",
        r#"
        const another = () => <Unrelated />;
        export default ((<Container render={() => <Deferred />}><Child /></Container>) satisfies unknown);
    "#,
    )?;
    let mut current = default_expression(&module)?;
    let mut wrappers = 0;
    loop {
        match &expression(&module, current)?.shape {
            ExpressionShape::Wrapper { expression, .. } => {
                wrappers += 1;
                current = *expression;
            }
            ExpressionShape::Jsx { node } => {
                let NodeKind::Element(element) = &module.node(*node).ok_or("节点引用失效")?.kind
                else {
                    return Err("根应为 JSX Element".into());
                };
                assert_eq!(element.name.text(), "Container");
                break;
            }
            shape => return Err(format!("非预期根结构：{shape:?}").into()),
        }
    }
    assert!(wrappers >= 2);
    let another = expression(&module, initializer(&module, "another")?)?;
    assert_eq!(another.shape, ExpressionShape::Opaque);
    assert!(
        !another.jsx_roots.is_empty(),
        "函数体仍保留语法索引，但不作为求值结果"
    );
    let exported_function = parse("function.tsx", "export default () => <Deferred />;")?;
    assert_eq!(
        expression(&exported_function, default_expression(&exported_function)?)?.shape,
        ExpressionShape::Opaque
    );
    Ok(())
}

#[test]
fn each_variable_has_its_own_pattern_initializer_and_source_span() -> TestResult {
    let module = parse(
        "values.tsx",
        r#"
        const first = 1, selected = (<Tree /> satisfies unknown), last = first;
        let empty: string;
        export default selected;
    "#,
    )?;
    let declaration = module
        .declarations
        .iter()
        .find(|declaration| declaration.kind == DeclarationKind::Const)
        .ok_or("缺少 const")?;
    assert_eq!(declaration.declarators.len(), 3);
    assert_eq!(
        module.raw(&declaration.declarators[0].span),
        Some("first = 1")
    );
    assert_eq!(
        module.raw(&declaration.declarators[1].span),
        Some("selected = (<Tree /> satisfies unknown)")
    );
    let selected = initializer(&module, "selected")?;
    assert!(matches!(
        expression(&module, selected)?.shape,
        ExpressionShape::Wrapper { .. }
    ));
    assert!(matches!(
        expression(&module, initializer(&module, "first")?)?.literal,
        Some(LiteralIR::Number { value: Some(1.0) })
    ));
    let exported = expression(&module, default_expression(&module)?)?;
    assert_eq!(exported.shape, ExpressionShape::Identifier);
    assert_eq!(
        exported
            .direct_identifier
            .as_ref()
            .map(|name| name.name.as_str()),
        Some("selected")
    );
    let empty = module
        .declarations
        .iter()
        .find(|declaration| declaration.kind == DeclarationKind::Let)
        .ok_or("缺少 let")?;
    assert_eq!(empty.declarators[0].initializer, None);
    Ok(())
}

#[test]
fn arrays_preserve_holes_and_spreads_while_objects_keep_order_and_accessors() -> TestResult {
    let module = parse(
        "collections.ts",
        r#"
        const x = 1, items = [x, , ...others, null];
        const values = { x, "中文": 2, [key]: 3, ...extra, get value() { return 4; }, set value(v) {}, method() {}, 2: 5, 3n: 6, __proto__: null };
    "#,
    )?;
    let ExpressionShape::Array { elements } =
        &expression(&module, initializer(&module, "items")?)?.shape
    else {
        return Err("应为数组".into());
    };
    assert_eq!(elements.len(), 4);
    assert!(elements[1].is_none());
    assert!(elements[2].as_ref().is_some_and(|item| item.spread));
    assert_eq!(
        module.raw(&elements[2].as_ref().ok_or("缺少 spread")?.span),
        Some("...others")
    );
    let ExpressionShape::Object { properties } =
        &expression(&module, initializer(&module, "values")?)?.shape
    else {
        return Err("应为对象".into());
    };
    assert_eq!(properties.len(), 10);
    assert!(matches!(
        properties[0].kind,
        ObjectPropertyKind::Value {
            shorthand: true,
            ..
        }
    ));
    assert!(matches!(
        properties[2].kind,
        ObjectPropertyKind::Value {
            key: PropertyKeyIR::Computed { .. },
            ..
        }
    ));
    assert!(matches!(
        properties[3].kind,
        ObjectPropertyKind::Spread { .. }
    ));
    for (index, expected) in [
        (4, ObjectMethodKind::Getter),
        (5, ObjectMethodKind::Setter),
        (6, ObjectMethodKind::Method),
    ] {
        assert!(
            matches!(&properties[index].kind, ObjectPropertyKind::Method { kind, .. } if *kind == expected)
        );
    }
    assert!(matches!(
        properties[7].kind,
        ObjectPropertyKind::Value {
            key: PropertyKeyIR::Number {
                value: Some(2.0),
                ..
            },
            ..
        }
    ));
    assert!(
        matches!(&properties[8].kind, ObjectPropertyKind::Value { key: PropertyKeyIR::BigInt { decimal, .. }, .. } if decimal == "3")
    );
    assert_eq!(module.raw(&properties[9].span), Some("__proto__: null"));
    Ok(())
}

#[test]
fn operators_members_and_templates_preserve_direct_links_and_utf16() -> TestResult {
    let invalid = Engine::new().parse_source(
        Path::new("operators.ts"),
        r#"
        const limit = -5 + 2 * 3, selected = ok ? settings.name : settings[key];
        const label = `\uD800-${limit}-尾`;
        const flags = !ok || count >= 1 && absent ?? fallback;
    "#,
        ParseOptions::default(),
    )?;
    // JS 禁止没有括号地混用 ?? 和 ||/&&，恢复出的 AST 不能误报为有效。
    assert!(invalid.has_errors());
    let module = parse(
        "operators.ts",
        r#"
        const limit = -5 + 2 * 3, selected = ok ? settings.name : settings[key];
        const label = `\uD800-${limit}-尾`;
        const flags = (!ok || count >= 1 && absent) ?? fallback;
    "#,
    )?;
    let ExpressionShape::Binary {
        operator,
        left,
        right,
    } = &expression(&module, initializer(&module, "limit")?)?.shape
    else {
        return Err("应为二元式".into());
    };
    assert_eq!(operator, "+");
    assert!(
        matches!(&expression(&module, *left)?.shape, ExpressionShape::Unary { operator, .. } if operator == "-")
    );
    assert!(
        matches!(&expression(&module, *right)?.shape, ExpressionShape::Binary { operator, .. } if operator == "*")
    );
    let ExpressionShape::Conditional {
        consequent,
        alternate,
        ..
    } = &expression(&module, initializer(&module, "selected")?)?.shape
    else {
        return Err("应为条件式".into());
    };
    assert!(
        matches!(&expression(&module, *consequent)?.shape, ExpressionShape::Member { property: MemberPropertyIR::Identifier(name), .. } if name.name == "name")
    );
    assert!(matches!(
        &expression(&module, *alternate)?.shape,
        ExpressionShape::Member {
            property: MemberPropertyIR::Computed { .. },
            ..
        }
    ));
    let ExpressionShape::Template {
        quasis,
        expressions,
    } = &expression(&module, initializer(&module, "label")?)?.shape
    else {
        return Err("应为模板字符串".into());
    };
    assert_eq!(quasis.len(), 2);
    assert_eq!(expressions.len(), 1);
    assert_eq!(quasis[0].raw, "\\uD800-");
    assert_eq!(quasis[0].cooked_utf16, Some(vec![0xd800, 45]));
    assert!(quasis[1].tail);
    Ok(())
}

#[test]
fn destructuring_retains_aliases_defaults_computed_keys_holes_and_rest() -> TestResult {
    let module = parse(
        "patterns.ts",
        r#"
        const { source: renamed = 3, plain = 4, [key]: nested, ...rest } = origin;
        const [first, , third = 7, ...tail] = sequence;
    "#,
    )?;
    let BindingPatternKind::Object { properties } =
        &module.declarations[0].declarators[0].pattern.kind
    else {
        return Err("应为对象绑定模式".into());
    };
    assert_eq!(properties.len(), 4);
    let BindingPropertyIR::Property {
        key: PropertyKeyIR::Identifier(key),
        value,
        shorthand,
        ..
    } = &properties[0]
    else {
        return Err("缺少别名属性".into());
    };
    assert_eq!(key.name, "source");
    assert!(!shorthand);
    let BindingPatternKind::Assignment { target, default } = &value.kind else {
        return Err("缺少默认值".into());
    };
    assert!(matches!(&target.kind, BindingPatternKind::Identifier(name) if name.name == "renamed"));
    assert_eq!(module.raw(&expression(&module, *default)?.span), Some("3"));
    assert!(matches!(
        &properties[1],
        BindingPropertyIR::Property {
            shorthand: true,
            value: BindingPatternIR {
                kind: BindingPatternKind::Assignment { .. },
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        &properties[2],
        BindingPropertyIR::Property {
            key: PropertyKeyIR::Computed { .. },
            ..
        }
    ));
    assert!(matches!(&properties[3], BindingPropertyIR::Rest { .. }));
    let BindingPatternKind::Array { elements } =
        &module.declarations[1].declarators[0].pattern.kind
    else {
        return Err("应为数组绑定模式".into());
    };
    assert_eq!(elements.len(), 4);
    assert!(elements[1].is_none());
    assert!(matches!(
        &elements[2].as_ref().ok_or("缺少第三项")?.kind,
        BindingPatternKind::Assignment { .. }
    ));
    assert!(matches!(
        &elements[3].as_ref().ok_or("缺少 rest")?.kind,
        BindingPatternKind::Rest { .. }
    ));
    Ok(())
}

#[test]
fn typescript_wrappers_are_explicit_and_calls_remain_opaque() -> TestResult {
    let module = parse(
        "wrappers.ts",
        r#"
        const asserted = <number>value, constant = <const>[1, 2];
        const instantiated = identity<number>, nonnull = value!, checked = value satisfies unknown;
        const called = identity(3), optional = source?.name;
    "#,
    )?;
    for (name, kind) in [
        ("asserted", ExpressionWrapperKind::TypeScriptTypeAssertion),
        ("constant", ExpressionWrapperKind::TypeScriptConstAssertion),
        (
            "instantiated",
            ExpressionWrapperKind::TypeScriptInstantiation,
        ),
        ("nonnull", ExpressionWrapperKind::TypeScriptNonNull),
        ("checked", ExpressionWrapperKind::TypeScriptSatisfies),
    ] {
        assert!(
            matches!(&expression(&module, initializer(&module, name)?)?.shape, ExpressionShape::Wrapper { kind: actual, .. } if *actual == kind)
        );
    }
    for name in ["called", "optional"] {
        assert_eq!(
            expression(&module, initializer(&module, name)?)?.shape,
            ExpressionShape::Opaque
        );
    }
    Ok(())
}

#[test]
fn expanded_ir_roundtrips_and_old_snapshots_remain_conservatively_opaque() -> TestResult {
    let module = parse(
        "old.tsx",
        "const value = {x: [1, , 3]}; export default (<Tree value={value} />);",
    )?;
    let encoded = serde_json::to_string(&module)?;
    assert_eq!(serde_json::from_str::<ModuleIR>(&encoded)?, module);
    let mut legacy = serde_json::to_value(&module)?;
    for value in legacy["expressions"]
        .as_array_mut()
        .ok_or("缺少表达式列表")?
    {
        value
            .as_object_mut()
            .ok_or("表达式应为对象")?
            .remove("shape");
    }
    for value in legacy["declarations"]
        .as_array_mut()
        .ok_or("缺少声明列表")?
    {
        value
            .as_object_mut()
            .ok_or("声明应为对象")?
            .remove("declarators");
    }
    let decoded: ModuleIR = serde_json::from_value(legacy)?;
    assert!(
        decoded
            .expressions
            .iter()
            .all(|expression| expression.shape == ExpressionShape::Opaque)
    );
    assert!(
        decoded
            .declarations
            .iter()
            .all(|declaration| declaration.declarators.is_empty())
    );
    assert_eq!(decoded.source, module.source);
    assert_eq!(decoded.nodes, module.nodes);
    Ok(())
}
