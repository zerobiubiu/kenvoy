//! 模块内绑定的真实语言验证，不引入任何业务标签语义。
use kengine::*;
use std::{error::Error, path::Path};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn parse(source: &str) -> TestResult<ModuleIR> {
    let parsed =
        Engine::new().parse_source(Path::new("semantic.tsx"), source, ParseOptions::default())?;
    assert!(!parsed.has_errors(), "{:?}", parsed.diagnostics);
    Ok(parsed.module.ok_or("应返回模块")?)
}

fn reference<'a>(
    m: &'a ModuleIR,
    marker: &str,
    name: &str,
    namespace: SymbolNamespace,
) -> &'a ReferenceIR {
    let start =
        m.source.find(marker).expect("源码存在标记") + marker.find(name).expect("标记存在标识符");
    m.semantic
        .references_at(start, start + name.len())
        .find(|r| r.namespace == namespace)
        .expect("对应引用")
}

fn symbol<'a>(m: &'a ModuleIR, r: &ReferenceIR) -> &'a SymbolIR {
    m.semantic
        .symbol(r.symbol.expect("引用已绑定"))
        .expect("符号存在")
}

#[test]
fn binds_shadowing_closure_and_function_var_hoisting() -> TestResult {
    let m = parse(
        "const item = 1; function outer(input: number) { before(later); { let item = input; const capture = () => item; var later = capture; } return item + later(); } item;",
    )?;
    let inner = symbol(&m, reference(&m, "=> item", "item", SymbolNamespace::Value));
    let outer = symbol(
        &m,
        reference(&m, "return item", "item", SymbolNamespace::Value),
    );
    assert_ne!(inner.id, outer.id);
    assert_eq!(
        m.semantic.scope(inner.scope).map(|s| s.kind),
        Some(ScopeKind::Block)
    );
    assert_eq!(outer.scope, ScopeId(0));
    let before = symbol(
        &m,
        reference(&m, "before(later)", "later", SymbolNamespace::Value),
    );
    let after = symbol(
        &m,
        reference(&m, "later()", "later", SymbolNamespace::Value),
    );
    assert_eq!(before.id, after.id);
    assert_eq!(before.kind, SymbolKind::Var);
    assert_eq!(
        m.semantic.scope(before.scope).map(|s| s.kind),
        Some(ScopeKind::Function)
    );
    assert!(
        reference(&m, "before(later)", "before", SymbolNamespace::Value)
            .symbol
            .is_none()
    );
    Ok(())
}

#[test]
fn distinguishes_value_type_and_generic_parameter_bindings() -> TestResult {
    let m = parse(
        "type Shape = { size: number }; const Shape = 7; function identity<T extends Shape>(input: T): T { const n = Shape; return input; } type Copy = typeof Shape;",
    )?;
    let ty = symbol(
        &m,
        reference(&m, "extends Shape", "Shape", SymbolNamespace::Type),
    );
    let value = symbol(
        &m,
        reference(&m, "n = Shape", "Shape", SymbolNamespace::Value),
    );
    assert_eq!(ty.kind, SymbolKind::TypeAlias);
    assert_eq!(value.kind, SymbolKind::Const);
    assert_ne!(ty.id, value.id);
    let generic = symbol(&m, reference(&m, "input: T", "T", SymbolNamespace::Type));
    assert_eq!(generic.kind, SymbolKind::TypeParameter);
    assert_eq!(
        symbol(&m, reference(&m, "): T", "T", SymbolNamespace::Type)).id,
        generic.id
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "typeof Shape", "Shape", SymbolNamespace::Value)
        )
        .id,
        value.id
    );
    assert!(!m.semantic.references.iter().any(|r| r.name == "size"));
    Ok(())
}

#[test]
fn records_import_bindings_and_jsx_component_roots_only() -> TestResult {
    let m = parse(
        "import View, { type Props, render as draw } from './view'; import * as Widgets from './widgets'; const result: Props = draw({}); export default <><View value={result}/><Widgets.Card/><native label='x'/></>;",
    )?;
    let view = symbol(&m, reference(&m, "<View", "View", SymbolNamespace::Value));
    assert_eq!(
        view.import.as_ref().and_then(|i| i.imported.as_deref()),
        Some("default")
    );
    let draw_ref = reference(&m, "draw({})", "draw", SymbolNamespace::Value);
    assert_eq!(draw_ref.kind, ReferenceKind::Call);
    assert_eq!(
        symbol(&m, draw_ref)
            .import
            .as_ref()
            .and_then(|i| i.imported.as_deref()),
        Some("render")
    );
    let widgets = symbol(
        &m,
        reference(&m, "<Widgets.Card", "Widgets", SymbolNamespace::Value),
    );
    assert!(widgets.import.as_ref().is_some_and(|i| i.namespace));
    assert!(
        !m.semantic
            .symbols
            .iter()
            .any(|s| s.name == "Props" && s.namespace == SymbolNamespace::Value)
    );
    assert!(
        reference(&m, "<native", "native", SymbolNamespace::Value)
            .symbol
            .is_none()
    );
    assert!(
        !m.semantic
            .references
            .iter()
            .any(|r| matches!(r.name.as_str(), "Card" | "label" | "value"))
    );
    Ok(())
}

#[test]
fn preserves_destructuring_defaults_computed_keys_and_write_kinds() -> TestResult {
    let m = parse(
        "const key = 'x', fallback = 3; let { [key]: selected = fallback, plain, ...rest } = source; selected = 4; selected += 1; plain++; rest[key] = selected;",
    )?;
    assert_eq!(
        symbol(&m, reference(&m, "[key]", "key", SymbolNamespace::Value)).kind,
        SymbolKind::Const
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "= fallback", "fallback", SymbolNamespace::Value)
        )
        .kind,
        SymbolKind::Const
    );
    assert_eq!(
        reference(&m, "selected = 4", "selected", SymbolNamespace::Value).kind,
        ReferenceKind::Write
    );
    assert_eq!(
        reference(&m, "selected +=", "selected", SymbolNamespace::Value).kind,
        ReferenceKind::ReadWrite
    );
    assert_eq!(
        reference(&m, "plain++", "plain", SymbolNamespace::Value).kind,
        ReferenceKind::ReadWrite
    );
    assert_eq!(
        reference(&m, "rest[key]", "rest", SymbolNamespace::Value).kind,
        ReferenceKind::Read
    );
    Ok(())
}

#[test]
fn class_methods_constructor_and_named_expressions_keep_local_names() -> TestResult {
    let m = parse(
        "class Box<T> { constructor(public value: T) {} read(): T { return this.value; } } const make = function inner(value: number) { return inner(value); }; const expr = class Local { self() { return Local; } }; const instance: Box<number> = new Box(1); inner; Local;",
    )?;
    assert_eq!(
        symbol(&m, reference(&m, "value: T", "T", SymbolNamespace::Type)).kind,
        SymbolKind::TypeParameter
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "return inner", "inner", SymbolNamespace::Value)
        )
        .kind,
        SymbolKind::Function
    );
    assert!(
        reference(&m, "inner;", "inner", SymbolNamespace::Value)
            .symbol
            .is_none()
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "return Local", "Local", SymbolNamespace::Value)
        )
        .kind,
        SymbolKind::Class
    );
    assert!(
        reference(&m, "Local;", "Local", SymbolNamespace::Value)
            .symbol
            .is_some()
    );
    let trailing = m
        .semantic
        .references
        .iter()
        .rev()
        .find(|r| r.name == "Local")
        .expect("尾部引用");
    assert!(trailing.symbol.is_none());
    assert_eq!(
        reference(&m, "new Box", "Box", SymbolNamespace::Value).kind,
        ReferenceKind::Construct
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "instance: Box", "Box", SymbolNamespace::Type)
        )
        .kind,
        SymbolKind::Class
    );
    Ok(())
}

#[test]
fn catch_loop_and_labels_do_not_leak_bindings() -> TestResult {
    let m = parse(
        "outer: for (let index = 0; index < 2; index++) { try { throw index; } catch (error) { use(error); break outer; } } index; error;",
    )?;
    assert!(!m.semantic.references.iter().any(|r| r.name == "outer"));
    assert_eq!(
        symbol(
            &m,
            reference(&m, "index <", "index", SymbolNamespace::Value)
        )
        .kind,
        SymbolKind::Let
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "use(error)", "error", SymbolNamespace::Value)
        )
        .kind,
        SymbolKind::CatchParameter
    );
    assert!(
        m.semantic
            .references
            .iter()
            .rev()
            .find(|r| r.name == "index")
            .expect("尾部引用")
            .symbol
            .is_none()
    );
    assert!(
        m.semantic
            .references
            .iter()
            .rev()
            .find(|r| r.name == "error")
            .expect("尾部引用")
            .symbol
            .is_none()
    );
    Ok(())
}

#[test]
fn merges_var_occurrences_and_tracks_separate_exports_without_foreign_names() -> TestResult {
    let m = parse(
        "var repeated = 1; var repeated = 2; export { repeated as renamed }; export { foreign as elsewhere } from './other'; export default function named() { return named; } named();",
    )?;
    let repeated = symbol(
        &m,
        reference(&m, "{ repeated as", "repeated", SymbolNamespace::Value),
    );
    assert_eq!(repeated.declarations.len(), 2);
    assert_eq!(
        reference(&m, "{ repeated as", "repeated", SymbolNamespace::Value).kind,
        ReferenceKind::Export
    );
    assert!(
        !m.semantic
            .references
            .iter()
            .any(|r| matches!(r.name.as_str(), "foreign" | "elsewhere" | "renamed"))
    );
    assert_eq!(
        symbol(
            &m,
            reference(&m, "named();", "named", SymbolNamespace::Value)
        )
        .scope,
        ScopeId(0)
    );
    Ok(())
}

#[test]
fn semantic_ids_and_unicode_spans_round_trip_deterministically() -> TestResult {
    let source = "\u{feff}const 名称 = '😀';\r\nconst view = () => 名称;";
    let m = parse(source)?;
    assert_eq!(m, parse(source)?);
    let reference = reference(&m, "=> 名称", "名称", SymbolNamespace::Value);
    assert_eq!(m.raw(&reference.span), Some("名称"));
    assert_eq!(reference.span.start_position.line, 2);
    let encoded = serde_json::to_string(&m)?;
    assert_eq!(serde_json::from_str::<ModuleIR>(&encoded)?, m);
    assert!(m.semantic.symbol(SymbolId(999)).is_none());
    let mut old = serde_json::to_value(&m)?;
    old.as_object_mut()
        .ok_or("模块应为对象")?
        .remove("semantic");
    assert_eq!(
        serde_json::from_value::<ModuleIR>(old)?.semantic,
        SemanticIR::default()
    );
    Ok(())
}

#[test]
fn jsx_names_are_generic_and_do_not_impose_a_framework_case_rule() -> TestResult {
    let m = parse(
        "const _Card = 1, $Card = 2, lowercase = 3; const output = <><_Card/><$Card/><lowercase/><unknown/></>;",
    )?;
    for (marker, name) in [
        ("<_Card", "_Card"),
        ("<$Card", "$Card"),
        ("<lowercase", "lowercase"),
    ] {
        let r = reference(&m, marker, name, SymbolNamespace::Value);
        assert_eq!(r.kind, ReferenceKind::Component);
        assert_eq!(symbol(&m, r).kind, SymbolKind::Const);
    }
    assert!(
        reference(&m, "<unknown", "unknown", SymbolNamespace::Value)
            .symbol
            .is_none()
    );
    Ok(())
}

#[test]
fn conditional_infer_and_signature_parameters_have_separate_type_scopes() -> TestResult {
    let m = parse(
        "type U = string; type Extract<T> = T extends Array<infer U> ? U : U; interface Contract { first<T>(value: T): T; second<T>(value: T): T; field: U; } type Mapped<T> = { [K in keyof T]: K };",
    )?;
    let true_u = symbol(&m, reference(&m, "? U", "U", SymbolNamespace::Type));
    let false_u = symbol(
        &m,
        reference(&m, ": U; interface", "U", SymbolNamespace::Type),
    );
    assert_eq!(true_u.kind, SymbolKind::TypeParameter);
    assert_eq!(false_u.kind, SymbolKind::TypeAlias);
    assert_ne!(true_u.id, false_u.id);
    let generics: Vec<_> = m
        .semantic
        .symbols
        .iter()
        .filter(|s| s.name == "T" && s.kind == SymbolKind::TypeParameter)
        .collect();
    assert_eq!(generics.len(), 4);
    assert_ne!(generics[1].scope, generics[2].scope);
    assert!(
        !m.semantic
            .references
            .iter()
            .any(|r| matches!(r.name.as_str(), "first" | "second" | "field"))
    );
    assert_eq!(
        symbol(&m, reference(&m, "]: K", "K", SymbolNamespace::Type)).kind,
        SymbolKind::TypeParameter
    );
    Ok(())
}

#[test]
fn jsx_parameter_defaults_cannot_see_body_declarations() -> TestResult {
    for name in ["Card", "_Card", "$Card", "lowercase"] {
        let source = format!(
            "const {name} = 1; function example(input=<{name}/>) {{ var {name}=2; return <{name}/>; }}"
        );
        let m = parse(&source)?;
        let refs: Vec<_> = m
            .semantic
            .references
            .iter()
            .filter(|r| r.name == name && r.kind == ReferenceKind::Component)
            .collect();
        assert_eq!(refs.len(), 2);
        assert_eq!(symbol(&m, refs[0]).scope, ScopeId(0));
        assert_ne!(refs[0].symbol, refs[1].symbol);
        let no_outer = parse(&format!(
            "const example = (input=<{name}/>) => {{ var {name}=2; return <{name}/>; }};"
        ))?;
        let refs: Vec<_> = no_outer
            .semantic
            .references
            .iter()
            .filter(|r| r.name == name && r.kind == ReferenceKind::Component)
            .collect();
        assert_eq!(refs.len(), 2);
        assert!(refs[0].symbol.is_none(), "默认值不可见函数体 {name}");
        assert!(refs[1].symbol.is_some(), "函数体可见自身 {name}");
    }
    Ok(())
}

#[test]
fn type_only_import_supports_typeof_without_a_runtime_binding() -> TestResult {
    let m = parse(
        "import type { value, callable } from './source'; type Value = typeof value; type Function = typeof callable; value(); callable();",
    )?;
    for name in ["value", "callable"] {
        let query = reference(&m, &format!("typeof {name}"), name, SymbolNamespace::Value);
        assert_eq!(query.kind, ReferenceKind::TypeQuery);
        assert_eq!(symbol(&m, query).namespace, SymbolNamespace::Type);
        assert!(
            !m.semantic
                .symbols
                .iter()
                .any(|s| s.name == name && s.namespace == SymbolNamespace::Value)
        );
        let call = reference(&m, &format!("{name}()"), name, SymbolNamespace::Value);
        assert!(call.symbol.is_none(), "type-only import 不产生可执行值绑定");
    }
    Ok(())
}
