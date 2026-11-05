//! 从 KEngine 的真实项目分析结果检查消费者行为，不执行 kenvoy 项目源码。
use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use kengine::{AnalyzeOptions, Engine, ProgramIR, ResolveOptions};

use super::validate_i18n;
use crate::domain::{i18n::I18nBundle, source::DslDiagnostic};

type TestResult = Result<(), Box<dyn Error>>;
static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Project(PathBuf);
impl Project {
    fn new(files: &[(&str, &str)]) -> Result<Self, Box<dyn Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "kenvoy-i18n-{}-{stamp}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        let project = Self(root);
        for (name, source) in files {
            let path = project.0.join(name);
            fs::create_dir_all(path.parent().ok_or("源码缺少目录")?)?;
            fs::write(path, source)?;
        }
        Ok(project)
    }

    fn analyze(&self) -> Result<ProgramIR, Box<dyn Error>> {
        let program = Engine::new().analyze_project(
            &self.0,
            AnalyzeOptions {
                entry_points: vec!["main.tsx".into()],
                resolve: ResolveOptions {
                    externals: ["@kenvoy/client".into(), "react".into(), "node:fs".into()].into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        assert!(!program.has_errors(), "{:?}", program.diagnostics);
        Ok(program)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn bundle(keys: &[&str]) -> I18nBundle {
    let mut bundle = I18nBundle::default();
    for messages in bundle.messages.values_mut() {
        for key in keys {
            messages.insert((*key).into(), format!("译文 {key}"));
        }
    }
    bundle
}

fn errors(source: &str, keys: &[&str]) -> Result<Vec<DslDiagnostic>, Box<dyn Error>> {
    let project = Project::new(&[("main.tsx", source)])?;
    Ok(validate_i18n(&project.analyze()?, &bundle(keys)).expect_err("应拒绝未完成的翻译配置"))
}

#[test]
fn missing_key_has_exact_source_and_all_language_suggestion_even_with_fallback() -> TestResult {
    let diagnostics = errors(
        "import { useI18n } from '@kenvoy/client';\nexport function Card() {\n  const { t: translate } = useI18n();\n  return translate('business.missing', {}, '旧文案');\n}",
        &[],
    )?;
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let error = &diagnostics[0];
    assert_eq!(error.code, "KV2201");
    assert_eq!(error.source.span.start_position.line, 4);
    assert_eq!(error.source.span.start_position.column, 20);
    assert!(error.source.path.ends_with("main.tsx"));
    assert!(error.message.contains("business.missing"));
    assert!(
        error.suggestion.contains("translations.json")
            && error.suggestion.contains("全部已配置语言")
    );
    assert!(error.suggestion.contains("fallback"));
    Ok(())
}

#[test]
fn complete_business_and_real_system_keys_are_available() -> TestResult {
    let project = Project::new(&[(
        "main.tsx",
        "import {useI18n} from '@kenvoy/client'; export function Card(){const {t}=useI18n(); return <p>{t('business.title')}{t('ui.cancel')}{t('KV4001')}{t('validation.required')}</p>}",
    )])?;
    let program = project.analyze()?;
    assert!(validate_i18n(&program, &bundle(&["business.title"])).is_ok());
    let mut incomplete = bundle(&["business.title"]);
    incomplete
        .messages
        .get_mut("en-US")
        .ok_or("缺少英语词典")?
        .clear();
    let diagnostics = validate_i18n(&program, &incomplete).expect_err("每个业务键须覆盖全部语言");
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("en-US"));
    assert_eq!(errors("import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); t('ui.not_a_system_key');", &[])?[0].code, "KV2201");
    Ok(())
}

#[test]
fn plurals_require_base_or_other_and_count_is_not_implied() -> TestResult {
    let project = Project::new(&[(
        "main.tsx",
        "import {useI18n} from '@kenvoy/client'; export function Card({count}:{count:number}){const {t}=useI18n(); const params={count}; return <p>{t('items',{count:1})}{t('items',params)}{t('title')}</p>}",
    )])?;
    assert!(validate_i18n(&project.analyze()?, &bundle(&["items.other", "title"])).is_ok());
    let source =
        "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); t('items',{count:1});";
    assert_eq!(errors(source, &["items.one"])?[0].code, "KV2201");
    let no_count = "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); t('items');";
    assert_eq!(errors(no_count, &["items.other"])?[0].code, "KV2201");
    let string_count =
        "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); t('items',{count:'one'});";
    assert_eq!(errors(string_count, &["items.other"])?[0].code, "KV2201");
    assert!(
        validate_i18n(
            &Project::new(&[("main.tsx", no_count)])?.analyze()?,
            &bundle(&["items"])
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn sdk_aliases_namespace_destructuring_and_typescript_wrappers_keep_identity() -> TestResult {
    let source = r#"import {useI18n as useMessages} from '@kenvoy/client';
import * as sdk from '@kenvoy/client';
export function Card() {
  const hook=(useMessages!); const context=(hook() as ReturnType<typeof useMessages>);
  const {t: translate}=context; const alias=(translate satisfies typeof translate);
  const namespace=sdk; const {useI18n: use}=namespace; const i18n=use();
  const direct=(i18n.t!); const {t: another}=i18n;
  return <p>{alias('known')}{direct('known')}{another('known')}{(sdk.useI18n()).t('known')}</p>;
}"#;
    let project = Project::new(&[("main.tsx", source)])?;
    assert!(validate_i18n(&project.analyze()?, &bundle(&["known"])).is_ok());
    let missing = source.replace("'known'", "'missing'");
    let diagnostics = errors(&missing, &["known"])?;
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    assert!(diagnostics.iter().all(|error| error.code == "KV2201"));
    Ok(())
}

#[test]
fn lexical_shadowing_comments_and_strings_are_not_translation_calls() -> TestResult {
    let source = r#"import {useI18n} from '@kenvoy/client';
import * as sdk from '@kenvoy/client';
const text="t('not.configured')";
// t('not.configured'); sdk.useI18n().t('not.configured')
function ordinary(t:(key:string)=>string) { return t('ordinary'); }
function fake(useI18n:any, sdk:any) { const {t}=useI18n(); return t('ordinary')+sdk.useI18n().t('ordinary'); }
export function Card() {
  const {t}=useI18n();
  { const t=(value:string)=>value; t('ordinary'); }
  return <p>{t('known')}{ordinary(value=>value)}{text}</p>;
}"#;
    assert!(
        validate_i18n(
            &Project::new(&[("main.tsx", source)])?.analyze()?,
            &bundle(&["known"])
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn finite_local_constants_templates_and_conditions_check_every_possible_key() -> TestResult {
    let source = r#"import {useI18n} from '@kenvoy/client';
export function Card({active}:{active:boolean}) {
 const {t}=useI18n(); const prefix='status';
 const suffix=active?'on':'off'; const key=`${prefix}.${suffix}`;
 return t((key as string), {}, '不能代替词典');
}"#;
    let project = Project::new(&[("main.tsx", source)])?;
    assert!(validate_i18n(&project.analyze()?, &bundle(&["status.on", "status.off"])).is_ok());
    let diagnostics = errors(source, &[])?;
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|error| error.message.contains("status.on"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|error| error.message.contains("status.off"))
    );
    Ok(())
}

#[test]
fn runtime_keys_and_escaped_or_mutated_translation_functions_are_not_silently_skipped() -> TestResult
{
    for source in [
        "import {useI18n} from '@kenvoy/client'; export function Card({key}:{key:string}){const {t}=useI18n(); return t(key,{},'fallback')}",
        "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); const key=(()=> 'known')(); t(key);",
        "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); ['known'].map(t);",
        "import {useI18n} from '@kenvoy/client'; const i18n=useI18n(); export function useMessages(){return i18n;}",
        "import {useI18n} from '@kenvoy/client'; let {t}=useI18n(); t=key=>key; t('known');",
        "import {useI18n} from '@kenvoy/client'; const {t}=useI18n(); const object={translate:t}; object.translate('known');",
    ] {
        let diagnostics = errors(source, &["known"])?;
        assert!(
            diagnostics.iter().any(|error| error.code == "KV2202"),
            "{source}: {diagnostics:?}"
        );
    }
    Ok(())
}

#[test]
fn reachable_modules_direct_sdk_calls_reexports_and_imported_keys_are_checked() -> TestResult {
    let project = Project::new(&[
        (
            "main.tsx",
            "import {Card} from './card'; export default <Card/>;",
        ),
        (
            "card.tsx",
            "import * as hooks from './hooks'; import {key} from './keys'; export function Card(){const {t:translate}=hooks.useMessages(); return translate(key)}",
        ),
        (
            "hooks.ts",
            "export {useI18n as useMessages} from '@kenvoy/client';",
        ),
        ("keys.ts", "export const key='cross.module';"),
    ])?;
    let program = project.analyze()?;
    assert!(validate_i18n(&program, &bundle(&["cross.module"])).is_ok());
    let diagnostics = validate_i18n(&program, &bundle(&[])).expect_err("跨模块别名不得绕过检查");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].source.path.ends_with("card.tsx"));
    assert!(diagnostics[0].message.contains("cross.module"));
    let direct = Project::new(&[
        ("main.tsx", "import './nested'; export default <p/>;"),
        (
            "nested.tsx",
            "import {useI18n} from '@kenvoy/client'; export function Nested(){const {t}=useI18n(); return t('nested.missing')};",
        ),
    ])?;
    let diagnostics =
        validate_i18n(&direct.analyze()?, &bundle(&[])).expect_err("所有已分析模块均须检查");
    assert!(
        diagnostics
            .iter()
            .any(|error| error.source.path.ends_with("nested.tsx")
                && error.message.contains("nested.missing"))
    );
    Ok(())
}

#[test]
fn analysis_never_executes_top_level_effects_or_functions() -> TestResult {
    let project = Project::new(&[("main.tsx", "")])?;
    let marker = project.0.join("marker.txt");
    fs::write(&marker, "不可改写")?;
    let path = serde_json::to_string(&marker.to_string_lossy())?;
    let source = format!(
        "import {{writeFileSync}} from 'node:fs'; import {{useI18n}} from '@kenvoy/client'; writeFileSync({path},'被执行'); throw new Error('不应执行'); export function Card(){{const {{t}}=useI18n(); return t('known')}}"
    );
    fs::write(project.0.join("main.tsx"), source)?;
    let program = project.analyze()?;
    assert!(validate_i18n(&program, &bundle(&["known"])).is_ok());
    assert_eq!(fs::read_to_string(marker)?, "不可改写");
    Ok(())
}

#[test]
fn bom_and_crlf_positions_refer_to_original_source() -> TestResult {
    let diagnostics = errors(
        "\u{feff}import {useI18n} from '@kenvoy/client';\r\nconst {t}=useI18n();\r\nt('missing');",
        &[],
    )?;
    assert_eq!(diagnostics[0].source.span.start_position.line, 3);
    assert_eq!(diagnostics[0].source.span.start_position.column, 3);
    Ok(())
}

#[test]
fn mutable_key_containers_cannot_hide_runtime_missing_translations() -> TestResult {
    for source in [
        "import {useI18n} from '@kenvoy/client'; const keys={title:'known'}; export function Card(){const {t}=useI18n(); keys.title='missing'; return t(keys.title)}",
        "import {useI18n} from '@kenvoy/client'; const keys={title:'known'}; function change(value:any){value.title='missing'} export function Card(){const {t}=useI18n(); change(keys); return t(keys.title)}",
    ] {
        let diagnostics = errors(source, &["known"])?;
        assert!(
            diagnostics.iter().any(|error| error.code == "KV2202"),
            "无法证明对象不可变时必须拒绝，不能只检查初始值：{diagnostics:?}"
        );
    }
    Ok(())
}
