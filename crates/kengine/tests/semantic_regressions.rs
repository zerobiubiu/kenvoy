//! 独立回归：名称位置、参数环境与 JSX 绑定不依赖业务标签。

use std::{error::Error, path::Path};

use kengine::{Engine, ModuleIR, ParseOptions, ReferenceIR, ReferenceKind, SymbolNamespace};

fn parse(source: &str) -> Result<ModuleIR, Box<dyn Error>> {
    let parsed =
        Engine::new().parse_source(Path::new("regression.tsx"), source, ParseOptions::default())?;
    assert!(!parsed.has_errors(), "{:?}", parsed.diagnostics);
    parsed.module.ok_or_else(|| "缺少解析结果".into())
}

fn reference_at<'a>(module: &'a ModuleIR, occurrence: &str, name: &str) -> &'a ReferenceIR {
    let start = module.source.find(occurrence).expect("测试片段存在");
    module
        .semantic
        .references_at(start, start + name.len())
        .find(|reference| reference.namespace == SymbolNamespace::Value)
        .expect("应记录该位置的值引用")
}

#[test]
fn jsx_names_preserve_underscore_dollar_and_member_bindings() -> Result<(), Box<dyn Error>> {
    let module = parse(
        "const _Panel=()=>null, $Panel=()=>null, library={Panel:_Panel}; export default <><_Panel/><$Panel/><library.Panel/><arbitrary /></>;",
    )?;
    for name in ["_Panel", "$Panel", "library"] {
        let start = module
            .source
            .find(&format!("<{name}"))
            .ok_or("缺少 JSX 名称")?
            + 1;
        let reference = module
            .semantic
            .references_at(start, start + name.len())
            .find(|reference| reference.kind == ReferenceKind::Component)
            .ok_or("缺少组件引用")?;
        let symbol = module
            .semantic
            .symbol(reference.symbol.ok_or("组件引用没有绑定")?)
            .ok_or("缺少符号")?;
        assert_eq!(symbol.name, name);
    }
    Ok(())
}

#[test]
fn parameter_default_uses_outer_environment_before_body_var() -> Result<(), Box<dyn Error>> {
    let module =
        parse("const value=1; function sample(arg=value) { var value=2; return value+arg; }")?;
    let default = reference_at(&module, "value)", "value");
    let body = reference_at(&module, "value+arg", "value");
    assert!(default.symbol.is_some());
    assert!(body.symbol.is_some());
    assert_ne!(
        default.symbol, body.symbol,
        "参数默认值不能误绑定函数体内 var"
    );
    Ok(())
}

#[test]
fn member_assignment_and_destructuring_only_reference_evaluated_names() -> Result<(), Box<dyn Error>>
{
    let module = parse(
        "const obj={key:1}, key='key'; obj[key] += 2; const {key: alias}=obj; const result={alias};",
    )?;
    let object = reference_at(&module, "obj[key]", "obj");
    let computed = reference_at(&module, "key]", "key");
    assert_eq!(object.kind, ReferenceKind::Read);
    assert_eq!(computed.kind, ReferenceKind::Read);
    assert!(object.symbol.is_some() && computed.symbol.is_some());
    let property_start = module.source.find("key: alias").ok_or("缺少解构字段")?;
    assert!(
        module
            .semantic
            .references_at(property_start, property_start + 3)
            .next()
            .is_none()
    );
    let shorthand = reference_at(&module, "alias};", "alias");
    assert!(shorthand.symbol.is_some(), "对象简写应关联解构创建的 alias");
    Ok(())
}
