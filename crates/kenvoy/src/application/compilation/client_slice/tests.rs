use std::path::Path;

use crate::{
    application::compilation::AnalyzeResult, infrastructure::javascript::SwcClientCodeTransformer,
};

use super::*;

fn analyze(program: &ProgramIR, entry: &Path, name: &str) -> AnalyzeResult {
    crate::application::compilation::analyze(program, entry, name, &SwcClientCodeTransformer)
}

fn client_slice(
    program: &ProgramIR,
    app: &ApplicationIR,
    reference: &ModuleReference,
) -> Result<ClientSlice, JsDiagnostic> {
    super::client_slice(program, app, reference, &SwcClientCodeTransformer)
}

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
    let result = analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试");
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
    let result = analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试");
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
        let result = analyze(
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
    let app = analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试")
        .application
        .ok_or("泛型 Action 分析失败")?;
    let slice = client_slice(&program, &app, &app.module_references[0])?;
    assert_ne!(slice.export_name, "__kenvoy_value");
    assert!(slice.code.contains("safe"));
    assert!(!slice.code.contains("<T>"));
    Ok(())
}

#[tokio::test]
async fn template_local_constants_are_captured_without_executing_the_entry_or_private_action()
-> TestResult {
    let fixture = Fixture::new(
        "import {Card} from './card'; import {secret} from './secret.server'; const prefix='外层';export function readPrefix(){return prefix;} export default function App(){const prefix='模板',label=prefix+'局部';return <app name='模板'><action name='secret' runtime='server' permission='admin' handler={secret}/><page path='/test'><Card onClick={()=>label+readPrefix()}/></page></app>;}",
        &[
            ("card.tsx", CARD),
            (
                "secret.server.ts",
                "export function secret(){throw new Error('不能进入客户端');}",
            ),
        ],
    )?;
    let program = fixture.program()?;
    let app = analyze(&program, &fixture.0.join("app.tsx").canonicalize()?, "测试")
        .application
        .ok_or("函数模板分析失败")?;
    let reference = app
        .module_references
        .iter()
        .find(|reference| reference.kind == ModuleReferenceKind::ClientExpression)
        .ok_or("缺少局部常量的客户端表达式")?;
    let slice = client_slice(&program, &app, reference)?;
    assert!(slice.imports.is_empty());
    let runtime = rquickjs::AsyncRuntime::new()?;
    let context = rquickjs::AsyncContext::full(&runtime).await?;
    let value = context
        .async_with(async |ctx| -> rquickjs::Result<String> {
            let (module, ready) = rquickjs::Module::declare(
                ctx.clone(),
                "template-local.mjs",
                slice.code.as_bytes(),
            )?
            .eval()?;
            ready.into_future::<()>().await?;
            let factory: rquickjs::Function = module.get(slice.export_name.as_str())?;
            let callback: rquickjs::Function = factory.call(())?;
            callback.call(())
        })
        .await?;
    assert_eq!(value, "模板局部外层");
    Ok(())
}

#[test]
fn template_local_aliases_cannot_hide_private_client_dependencies() -> TestResult {
    let fixture = Fixture::new(
        "import {Card} from './card'; import {secret} from './secret.server'; export default ()=>{const callback=secret;return <page path='/test'><Card onClick={()=>callback()}/></page>;};",
        &[
            ("card.tsx", CARD),
            (
                "secret.server.ts",
                "export function secret(){return 'PRIVATE';}",
            ),
        ],
    )?;
    let result = analyze(
        &fixture.program()?,
        &fixture.0.join("app.tsx").canonicalize()?,
        "测试",
    );
    assert!(result.application.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|error| error.code == "KV2101")
    );
    Ok(())
}
