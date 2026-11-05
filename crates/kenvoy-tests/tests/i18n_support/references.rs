//! 显式翻译调用在脚本、迁移和监听前校验，复用真实 CLI 的有界进程夹具。

use std::{fs, time::Duration};

use kenvoy_tests::{Fixture, TestResult, require_success};
use serde_json::{Value, json};

use super::support::{command, project};

fn add_translation_call(fixture: &Fixture) -> TestResult {
    fs::write(
        fixture.project().join("src/Headline.tsx"),
        "import { useI18n } from '@kenvoy/client';\nexport function Headline() {\n  const { t: translate } = useI18n();\n  return <h2>{translate('business.headline', {}, '不能掩盖缺失键')}</h2>;\n}\n",
    )?;
    let entry = fixture.project().join("src/app.tsx");
    let source = fs::read_to_string(&entry)?;
    let source = source.replace(
        "<page path=\"/customers\" title=\"客户登记\">",
        "<page path=\"/customers\" title=\"客户登记\"><Headline />",
    );
    fs::write(
        entry,
        format!("import {{ Headline }} from './Headline';\n{source}"),
    )?;
    Ok(())
}

#[test]
fn missing_explicit_key_rejects_check_build_dev_and_run_without_touching_database() -> TestResult {
    let fixture = project("i18n-explicit-reference")?;
    add_translation_call(&fixture)?;
    let translations = fs::read(fixture.project().join("translations.json"))?;
    let migrations = fs::read(fixture.project().join("migrations/manifest.json"))?;
    let database = fixture.project().join("data/app.sqlite");
    for existing in [false, true] {
        let marker = b"owned-existing-database-must-not-be-opened\0";
        if existing {
            fs::write(&database, marker)?;
        }
        for operation in ["check", "build", "dev", "run"] {
            let output = fixture.command_with(
                &["--project", "app", operation],
                "",
                Duration::from_secs(10),
            )?;
            assert!(!output.status.success(), "{operation} 不得执行缺词项目");
            let diagnostic = String::from_utf8(output.stderr)?;
            assert!(diagnostic.contains("KV2201"), "{operation}: {diagnostic}");
            assert!(diagnostic.contains("business.headline"), "{diagnostic}");
            assert!(diagnostic.contains("Headline.tsx"), "{diagnostic}");
            assert_eq!(
                fs::read(fixture.project().join("translations.json"))?,
                translations
            );
            assert_eq!(
                fs::read(fixture.project().join("migrations/manifest.json"))?,
                migrations
            );
            if existing {
                assert_eq!(fs::read(&database)?, marker);
            } else {
                assert!(!database.exists());
            }
            assert!(!fixture.project().join("dist/manifest.json").exists());
            assert!(!fixture.project().join(".kenvoy/client-toolchain").exists());
        }
    }
    Ok(())
}

#[test]
fn configuring_all_languages_unblocks_a_previously_missing_reference() -> TestResult {
    let fixture = project("i18n-reference-recovery")?;
    add_translation_call(&fixture)?;
    assert!(!command(&fixture, &["check"])?.status.success());
    let path = fixture.project().join("translations.json");
    let mut document: Value = serde_json::from_slice(&fs::read(&path)?)?;
    document["business.headline"] = json!({"cn":"标题","en":"Headline","ko":"제목"});
    fs::write(path, serde_json::to_vec(&document)?)?;
    require_success(&command(&fixture, &["check"])?)?;
    assert!(!fixture.project().join("data/app.sqlite").exists());
    Ok(())
}
