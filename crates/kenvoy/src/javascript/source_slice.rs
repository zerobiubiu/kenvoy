//! 从声明模块提取浏览器所需的词法依赖，不把应用默认导出作为浏览器入口。
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

use kengine::{
    DeclarationKind, ModuleBindingTarget, ModuleIR, ProgramIR, ReferenceKind, ResolvedModule,
    ScopeId, SourceSpan, SymbolId, SymbolKind, SymbolNamespace,
};

use super::JsDiagnostic;
use crate::{
    application::{ApplicationIR, ModuleReference, ModuleReferenceKind},
    dsl::SourceLocation,
};

#[derive(Debug, Clone)]
pub enum ClientImportTarget {
    File(PathBuf),
    External(String),
}

#[derive(Debug, Clone)]
pub struct ClientSlice {
    pub code: String,
    pub export_name: String,
    pub imports: BTreeMap<String, ClientImportTarget>,
    pub source: SourceLocation,
}

pub fn client_slice(
    program: &ProgramIR,
    app: &ApplicationIR,
    reference: &ModuleReference,
) -> Result<ClientSlice, JsDiagnostic> {
    if !app
        .module_references
        .iter()
        .any(|candidate| candidate == reference)
    {
        return Err(failure(reference, "客户端引用不在已验证的应用注册表内"));
    }
    slice(program, reference)
}

/// DSL 边界检查与真正构建使用相同的依赖收集，不以整个声明模块的依赖替代表达式闭包。
pub(crate) fn slice(
    program: &ProgramIR,
    reference: &ModuleReference,
) -> Result<ClientSlice, JsDiagnostic> {
    let module = program
        .modules
        .iter()
        .find(|module| module.path == reference.module)
        .ok_or_else(|| failure(reference, "找不到表达式所属的源码模块"))?;
    let mut collector = Collector {
        program,
        module,
        reference,
        seen: BTreeSet::new(),
        pending: VecDeque::new(),
        pieces: BTreeMap::new(),
        imports: BTreeMap::new(),
        import_code: BTreeMap::new(),
    };
    let result = if reference.kind == ModuleReferenceKind::ClientExpression {
        let span = reference
            .expression
            .as_ref()
            .ok_or_else(|| failure(reference, "表达式缺少源码范围"))?;
        collector.capture(span)?;
        format!("() => ({})", raw(module, span, reference)?)
    } else {
        let targets = program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == reference.module
                    && export.name == reference.export
                    && export.namespace == SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .ok_or_else(|| failure(reference, "无法定位客户端导出"))?;
        match targets {
            [
                ModuleBindingTarget::Symbol {
                    module: owner,
                    symbol,
                },
            ] if owner == &module.path => {
                collector.pending.push_back(*symbol);
                module
                    .semantic
                    .symbol(*symbol)
                    .ok_or_else(|| failure(reference, "客户端导出的符号无效"))?
                    .name
                    .clone()
            }
            [
                ModuleBindingTarget::Expression {
                    module: owner,
                    expression,
                },
            ] if owner == &module.path => {
                let expression = module
                    .expression(*expression)
                    .ok_or_else(|| failure(reference, "客户端默认导出表达式缺失"))?;
                collector.capture(&expression.span)?;
                raw(module, &expression.span, reference)?.to_owned()
            }
            [
                ModuleBindingTarget::Declaration {
                    module: owner,
                    span,
                },
            ] if owner == &module.path => {
                collector.capture(span)?;
                raw(module, span, reference)?
                    .trim_start_matches("export ")
                    .trim_start_matches("default ")
                    .to_owned()
            }
            _ => {
                return Err(failure(
                    reference,
                    "声明文件转导或存在歧义的组件请从独立 UI 模块直接导入",
                ));
            }
        }
    };
    while let Some(symbol) = collector.pending.pop_front() {
        collector.include(symbol)?;
    }
    let mut source = String::new();
    for code in collector.import_code.values() {
        source.push_str(code);
        source.push('\n');
    }
    for code in collector.pieces.values() {
        source.push_str(code);
        source.push('\n');
    }
    let mut export_name = "__kenvoy_value".to_owned();
    while module
        .semantic
        .symbols
        .iter()
        .any(|symbol| symbol.name == export_name)
    {
        export_name.push('_');
    }
    source.push_str(&format!("export const {export_name} = ({result});\n"));
    let jsx = matches!(
        module
            .path
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("tsx" | "jsx")
    );
    let code = super::compiler::transpile(
        &source,
        if jsx {
            "kenvoy-client-slice.tsx"
        } else {
            "kenvoy-client-slice.ts"
        },
        jsx,
    )?;
    Ok(ClientSlice {
        code,
        export_name,
        imports: collector.imports,
        source: reference.source.clone(),
    })
}

struct Collector<'a> {
    program: &'a ProgramIR,
    module: &'a ModuleIR,
    reference: &'a ModuleReference,
    seen: BTreeSet<SymbolId>,
    pending: VecDeque<SymbolId>,
    pieces: BTreeMap<usize, String>,
    imports: BTreeMap<String, ClientImportTarget>,
    import_code: BTreeMap<usize, String>,
}

impl Collector<'_> {
    fn capture(&mut self, span: &SourceSpan) -> Result<(), JsDiagnostic> {
        for expression in &self.module.expressions {
            if within(&expression.span, span)
                && let Some(text) = self.module.raw(&expression.span)
            {
                let text = text.trim_start();
                if text.starts_with("import.meta")
                    || (expression.kind == "Call"
                        && text
                            .strip_prefix("import")
                            .and_then(|rest| rest.chars().next())
                            .is_some_and(|ch| ch == '(' || ch == '/' || ch.is_whitespace()))
                {
                    return Err(failure(self.reference, "客户端切片不允许动态模块加载"));
                }
            }
        }
        for reference in &self.module.semantic.references {
            if !within(&reference.span, span)
                || reference.namespace != SymbolNamespace::Value
                || matches!(
                    reference.kind,
                    ReferenceKind::Type | ReferenceKind::TypeQuery | ReferenceKind::Export
                )
            {
                continue;
            }
            let Some(symbol_id) = reference.symbol else {
                if matches!(reference.name.as_str(), "require" | "eval" | "Function") {
                    return Err(failure(
                        self.reference,
                        "客户端切片不允许未建模的代码或模块加载",
                    ));
                }
                continue;
            };
            let symbol = self
                .module
                .semantic
                .symbol(symbol_id)
                .ok_or_else(|| failure(self.reference, "词法引用失效"))?;
            if symbol
                .declarations
                .iter()
                .any(|declaration| within(declaration, span))
            {
                continue;
            }
            if symbol.scope != ScopeId(0) {
                return Err(failure(
                    self.reference,
                    "客户端表达式捕获了声明模块之外的局部作用域",
                ));
            }
            self.pending.push_back(symbol_id);
        }
        Ok(())
    }

    fn include(&mut self, id: SymbolId) -> Result<(), JsDiagnostic> {
        if !self.seen.insert(id) {
            return Ok(());
        }
        if self.seen.len() > 1024 {
            return Err(failure(self.reference, "客户端切片依赖超过 1024 个声明"));
        }
        let symbol = self
            .module
            .semantic
            .symbol(id)
            .ok_or_else(|| failure(self.reference, "找不到词法符号"))?;
        if let Some(import) = &symbol.import {
            let specifier = String::from_utf16(&import.source_utf16)
                .map_err(|_| failure(self.reference, "模块说明符不能转换为 UTF-8"))?;
            let declaration = self
                .module
                .imports
                .iter()
                .find(|declaration| {
                    declaration
                        .specifiers
                        .iter()
                        .any(|item| item.local.name == symbol.name && !item.type_only)
                })
                .ok_or_else(|| failure(self.reference, "客户端引用缺少普通 ESM 导入声明"))?;
            if declaration.attributes.is_some() || declaration.type_only {
                return Err(failure(
                    self.reference,
                    "客户端切片不支持导入属性或仅类型导入",
                ));
            }
            let target = self
                .program
                .module_graph
                .edges
                .iter()
                .find(|edge| {
                    edge.importer == self.module.path
                        && edge.specifier == specifier
                        && matches!(edge.kind, kengine::DependencyKind::Import)
                })
                .and_then(|edge| edge.target.as_ref())
                .ok_or_else(|| failure(self.reference, "无法解析客户端切片依赖"))?;
            let target = match target {
                ResolvedModule::File { path } => ClientImportTarget::File(path.clone()),
                ResolvedModule::External { specifier } => {
                    ClientImportTarget::External(specifier.clone())
                }
            };
            self.imports.insert(specifier.clone(), target);
            let quoted = serde_json::to_string(&specifier)
                .map_err(|_| failure(self.reference, "模块说明符序列化失败"))?;
            let clause = if import.namespace {
                format!("* as {}", symbol.name)
            } else if import.imported.as_deref() == Some("default") {
                symbol.name.clone()
            } else {
                let imported = serde_json::to_string(
                    import
                        .imported
                        .as_deref()
                        .ok_or_else(|| failure(self.reference, "导入名称缺失"))?,
                )
                .map_err(|_| failure(self.reference, "导入名称序列化失败"))?;
                format!("{{ {imported} as {} }}", symbol.name)
            };
            self.import_code
                .insert(import.span.start, format!("import {clause} from {quoted};"));
            return Ok(());
        }
        if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Const) {
            return Err(failure(
                self.reference,
                format!("客户端切片不支持捕获可变或复杂声明 {}", symbol.name),
            ));
        }
        let declaration = self
            .module
            .declarations
            .iter()
            .filter(|declaration| {
                symbol
                    .declarations
                    .iter()
                    .any(|span| within(span, &declaration.span))
            })
            .min_by_key(|declaration| declaration.span.end - declaration.span.start)
            .ok_or_else(|| failure(self.reference, "找不到客户端依赖的声明结构"))?;
        if declaration.kind == DeclarationKind::Function {
            let code = raw(self.module, &declaration.span, self.reference)?
                .trim_start_matches("export ")
                .trim_start_matches("default ")
                .to_owned();
            self.pieces.insert(declaration.span.start, code);
            self.capture(&declaration.span.clone())?;
        } else if declaration.kind == DeclarationKind::Const {
            let declarator = declaration
                .declarators
                .iter()
                .find(|item| {
                    symbol
                        .declarations
                        .iter()
                        .any(|span| within(span, &item.pattern.span))
                })
                .ok_or_else(|| failure(self.reference, "常量依赖缺少初始化结构"))?;
            if !matches!(
                declarator.pattern.kind,
                kengine::BindingPatternKind::Identifier(_)
            ) {
                return Err(failure(
                    self.reference,
                    "声明模块中的解构依赖请移到独立 UI 模块",
                ));
            }
            let initializer = declarator
                .initializer
                .and_then(|id| self.module.expression(id))
                .ok_or_else(|| failure(self.reference, "客户端常量必须具有初始值"))?;
            if !matches!(initializer.kind.as_str(), "Arrow" | "Function")
                && self.module.expressions.iter().any(|expr| {
                    within(&expr.span, &initializer.span)
                        && matches!(
                            expr.kind.as_str(),
                            "Call" | "New" | "Await" | "Yield" | "Assignment" | "Update"
                        )
                })
            {
                return Err(failure(
                    self.reference,
                    "声明模块的客户端依赖包含无法证明无副作用的初始化",
                ));
            }
            self.pieces.insert(
                declarator.span.start,
                format!(
                    "const {};",
                    raw(self.module, &declarator.span, self.reference)?
                ),
            );
            self.capture(&declarator.span.clone())?;
        } else {
            return Err(failure(
                self.reference,
                "声明模块中的组件依赖请拆分为独立 UI 模块",
            ));
        }
        Ok(())
    }
}

fn within(inner: &SourceSpan, outer: &SourceSpan) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}
fn raw<'a>(
    module: &'a ModuleIR,
    span: &SourceSpan,
    reference: &ModuleReference,
) -> Result<&'a str, JsDiagnostic> {
    module
        .raw(span)
        .ok_or_else(|| failure(reference, "源码范围不是有效的 UTF-8 字节区间"))
}
fn failure(reference: &ModuleReference, message: impl Into<String>) -> JsDiagnostic {
    JsDiagnostic::new(
        "KV6101",
        message,
        Some(reference.source.clone()),
        "把复杂组件或函数移到独立 client/shared 模块，使用静态 ESM 导入",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(source: &str, files: &[(&str, &str)]) -> Result<Self, Box<dyn std::error::Error>> {
            let root = std::env::temp_dir().join(format!(
                "kenvoy-slice-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            std::fs::create_dir(&root)?;
            std::fs::write(root.join("app.tsx"), source)?;
            for (name, text) in files {
                std::fs::write(root.join(name), text)?;
            }
            Ok(Self(root))
        }
        fn program(&self) -> Result<ProgramIR, Box<dyn std::error::Error>> {
            let program = kengine::Engine::new().analyze_project(
                &self.0,
                kengine::AnalyzeOptions {
                    entry_points: vec!["app.tsx".into()],
                    ..Default::default()
                },
            )?;
            assert!(!program.has_errors(), "{:?}", program.diagnostics);
            Ok(program)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.0.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
    const CARD: &str = "export function Card(props:any){return <div>{props.value}</div>}";

    #[test]
    fn inline_expression_keeps_only_lexical_client_captures() -> TestResult {
        let fixture = Fixture::new(
            "import {Card} from './card'; import {secret} from './secret.server'; const label:string='客户端'; const ignored='私有未引用常量'; export default <app name='测试'><action name='secret' runtime='server' permission='admin' handler={secret}/><page path='/test'><Card onClick={() => label + '内容'}/></page></app>;",
            &[
                ("card.tsx", CARD),
                (
                    "secret.server.ts",
                    "export function secret(){return '不得进入客户端'}",
                ),
            ],
        )?;
        let program = fixture.program()?;
        let result =
            crate::dsl::analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试");
        assert!(result.application.is_some(), "{:?}", result.diagnostics);
        let app = result.application.ok_or("缺少应用")?;
        let reference = app
            .module_references
            .iter()
            .find(|r| r.kind == ModuleReferenceKind::ClientExpression)
            .ok_or("缺少客户端表达式")?;
        let slice = client_slice(&program, &app, reference)?;
        assert!(slice.code.contains("客户端"));
        assert!(!slice.code.contains("私有未引用常量"));
        assert!(!slice.code.contains("secret"));
        assert!(!slice.code.contains(": string"));
        assert!(slice.imports.is_empty());
        Ok(())
    }

    #[test]
    fn exported_local_component_selects_individual_constants_and_imports() -> TestResult {
        let fixture = Fixture::new(
            "import {Card,unused} from './card'; const title='标题', secret='私有'; export function Local<T>() {return <Card value={title}/>;} export default <page path='/local'><Local/></page>;",
            &[(
                "card.tsx",
                "export function Card(props:any){return <div/>;} export const unused='未使用';",
            )],
        )?;
        let program = fixture.program()?;
        let result =
            crate::dsl::analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试");
        assert!(result.application.is_some(), "{:?}", result.diagnostics);
        let app = result.application.ok_or("缺少应用")?;
        let slice = client_slice(&program, &app, &app.module_references[0])?;
        assert!(slice.code.contains("<Card"));
        assert!(slice.code.contains("标题"));
        assert!(!slice.code.contains("私有"));
        assert!(!slice.code.contains("unused"));
        assert!(!slice.code.contains("<T>"));
        assert_eq!(slice.imports.len(), 1);
        Ok(())
    }

    #[test]
    fn captured_private_module_and_mixed_shared_server_roots_are_rejected() -> TestResult {
        for source in [
            "import {Card} from './card'; import {secret} from './secret.server'; export default <page path='/test'><Card onClick={()=>secret()}/></page>;",
            "import {secret,shared} from './mixed'; export default <app name='拒绝'><action name='secret' handler={secret} runtime='server' permission='admin'/><action name='shared' handler={shared} runtime='shared' permission='anonymous'/></app>;",
        ] {
            let fixture = Fixture::new(
                source,
                &[
                    ("card.tsx", CARD),
                    ("secret.server.ts", "export function secret(){return 1;}"),
                    (
                        "mixed.ts",
                        "export function secret(){return '私密'} export function shared(){return 1;}",
                    ),
                ],
            )?;
            let result = crate::dsl::analyze(
                &fixture.program()?,
                &fixture.0.join("app.tsx").canonicalize()?,
                "测试",
            );
            assert!(result.application.is_none());
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|error| error.code == "KV2101"),
                "{:?}",
                result.diagnostics
            );
        }
        Ok(())
    }

    #[test]
    fn plain_typescript_generics_and_generated_export_names_are_preserved() -> TestResult {
        let fixture = Fixture::new(
            "import {identity} from './identity'; export default <app name='泛型'><action name='identity' runtime='client' permission='anonymous' handler={identity}/></app>;",
            &[(
                "identity.ts",
                "const __kenvoy_value='safe'; export const identity = <T>(value:T)=>({value,label:__kenvoy_value});",
            )],
        )?;
        let program = fixture.program()?;
        let app = crate::dsl::analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试")
            .application
            .ok_or("泛型 Action 分析失败")?;
        let slice = client_slice(&program, &app, &app.module_references[0])?;
        assert_ne!(slice.export_name, "__kenvoy_value");
        assert!(slice.code.contains("safe"));
        assert!(!slice.code.contains("<T>"));
        Ok(())
    }
}
