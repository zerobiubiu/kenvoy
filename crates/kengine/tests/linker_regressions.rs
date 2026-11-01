//! 导入导出边界的独立回归，不以模块存在替代绑定正确性。

use kengine::*;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "kengine-link-review-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root)?;
        let project = Self(root);
        for (name, source) in files {
            fs::write(project.0.join(name), source)?;
        }
        Ok(project)
    }
    fn analyze(&self) -> TestResult<ProgramIR> {
        Ok(Engine::new().analyze_project(
            &self.0,
            AnalyzeOptions {
                entry_points: vec!["main.ts".into()],
                ..Default::default()
            },
        )?)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // 只删除本测试生成的临时目录。
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn ambiguous_explicit_reexport_is_an_error_without_an_import_consumer() -> TestResult {
    let project = Project::new(&[
        ("main.ts", "export { value as chosen } from './barrel';"),
        ("barrel.ts", "export * from './a'; export * from './b';"),
        ("a.ts", "export const value = 1;"),
        ("b.ts", "export const value = 2;"),
    ])?;
    let program = project.analyze()?;
    assert!(
        program
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "KE2102" && diagnostic.path.ends_with("main.ts")),
        "显式导出歧义必须在自身报告：{:?}",
        program.diagnostics
    );
    Ok(())
}

#[test]
fn unrepresentable_module_export_names_are_not_silently_dropped() -> TestResult {
    let project = Project::new(&[
        (
            "main.ts",
            r#"import { "\uD800" as value } from './source'; export { value };"#,
        ),
        (
            "source.ts",
            r#"const source = 1; export { source as "\uD800" };"#,
        ),
    ])?;
    let program = project.analyze()?;
    assert!(
        program.has_errors(),
        "不能把无法表示的导入导出名称当作成功的空绑定"
    );
    assert!(
        program
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span.is_some()),
        "应保留错误位置"
    );
    Ok(())
}

#[test]
fn unrepresentable_reexport_name_cannot_fall_back_to_default() -> TestResult {
    let project = Project::new(&[
        (
            "main.ts",
            r#"export { "\uD800" as chosen } from './source';"#,
        ),
        ("source.ts", "export default 7;"),
    ])?;
    let program = project.analyze()?;
    assert!(
        program.has_errors(),
        "无损转换失败的名称必须拒绝，不能误选默认导出"
    );
    assert!(!program.module_bindings.exports.iter().any(
        |export| export.module.ends_with("main.ts")
            && export.name == "chosen"
            && !export.targets.is_empty()
    ));
    Ok(())
}

#[test]
fn type_only_reexports_and_default_expression_indirection_keep_their_boundaries() -> TestResult {
    let project = Project::new(&[
        (
            "main.ts",
            "import value from './source'; export type { Shape } from './types'; export default value;",
        ),
        ("source.ts", "const result = 7; export default result;"),
        ("types.ts", "export interface Shape { id: string }"),
    ])?;
    let program = project.analyze()?;
    assert!(!program.has_errors(), "{:?}", program.diagnostics);
    let main = program
        .modules
        .iter()
        .find(|module| module.path.ends_with("main.ts"))
        .ok_or("缺少入口")?;
    assert!(
        !program
            .module_bindings
            .exports
            .iter()
            .any(|export| export.module == main.path
                && export.name == "Shape"
                && export.namespace == SymbolNamespace::Value)
    );
    let default = program
        .module_bindings
        .exports
        .iter()
        .find(|export| {
            export.module == main.path
                && export.name == "default"
                && export.namespace == SymbolNamespace::Value
        })
        .ok_or("缺少默认导出")?;
    let [ModuleBindingTarget::Expression { module, expression }] = default.targets.as_slice()
    else {
        return Err("默认值应保留表达式间接引用".into());
    };
    assert_eq!(module, &main.path);
    let expression = main.expression(*expression).ok_or("表达式 ID 失效")?;
    assert_eq!(expression.shape, ExpressionShape::Identifier);
    let reference = main
        .semantic
        .references_at(expression.span.start, expression.span.end)
        .find(|reference| reference.namespace == SymbolNamespace::Value)
        .ok_or("缺少默认导出引用")?;
    let symbol = reference.symbol.ok_or("默认导入未绑定")?;
    let import = program
        .module_bindings
        .imports
        .iter()
        .find(|import| import.module == main.path && import.symbol == symbol)
        .ok_or("缺少跨模块绑定")?;
    assert!(
        matches!(import.targets.as_slice(), [ModuleBindingTarget::Expression { module, .. }] if module.ends_with("source.ts"))
    );
    Ok(())
}
