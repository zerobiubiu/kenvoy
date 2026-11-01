//! Kenvoy 仅消费 KEngine 公共 API，不接触 SWC 或赋予引擎业务语义。

use std::{error::Error, path::Path};

use kengine::{Engine, ModuleIR, ParseOptions};

#[test]
fn binary_host_consumes_versioned_generic_ir_through_public_api() -> Result<(), Box<dyn Error>> {
    let source = r#"
        import type { Input } from './input';
        export function Widget<T>(value: T) { return <span>{String(value)}</span>; }
        export default <app name="任意 JSX"><page path="/records"><text name="name" required /></page></app>;
    "#;
    let parsed =
        Engine::new().parse_source(Path::new("boundary.tsx"), source, ParseOptions::default())?;
    assert!(!parsed.has_errors(), "语言合法时应能被宿主消费");
    let module = parsed.module.ok_or("缺少通用模块结果")?;
    assert_eq!(module.format_version, 1);
    assert_eq!(module.source, source);
    assert!(!module.nodes.is_empty());
    assert!(!module.imports.is_empty());
    let json = serde_json::to_string(&module)?;
    let restored: ModuleIR = serde_json::from_str(&json)?;
    assert_eq!(restored.source, source);
    assert_eq!(restored.nodes.len(), module.nodes.len());
    Ok(())
}

#[test]
fn host_rejects_language_errors_before_any_business_interpretation() -> Result<(), Box<dyn Error>> {
    let parsed = Engine::new().parse_source(
        Path::new("broken.tsx"),
        "export default <broken>;",
        ParseOptions::default(),
    )?;
    assert!(parsed.has_errors());
    assert!(!parsed.get_diagnostics().is_empty());
    Ok(())
}
