use super::*;

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
