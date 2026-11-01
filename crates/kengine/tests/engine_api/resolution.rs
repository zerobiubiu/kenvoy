use super::*;

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
