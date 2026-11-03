//! 根译文在源码、产物和数据库之外独立验证；拒绝后可原地恢复。

use std::{collections::BTreeMap, fs, time::Duration};

use kenvoy_tests::{Fixture, TestResult, require_success};
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::support::{
    Session, applied, archive_sources, build, command, form, manifest, no_node, project, read_db,
    schema, stored, version,
};

fn rejected_start(fixture: &Fixture, mode: &str, extra: &[&str]) -> TestResult<String> {
    let mut args = vec!["--project", "app", mode];
    args.extend_from_slice(extra);
    let output = fixture.command_with(&args, "", Duration::from_secs(10))?;
    assert!(!output.status.success(), "非法译文不能启动 {mode}");
    let diagnostic = String::from_utf8(output.stderr)?;
    assert!(!diagnostic.trim().is_empty(), "启动拒绝必须提供诊断");
    Ok(diagnostic)
}

#[tokio::test]
async fn missing_or_invalid_translations_stop_dev_and_run_before_database_or_build() -> TestResult {
    let fixture = project("i18n-startup-gate")?;
    let path = fixture.project().join("translations.json");
    let original = fs::read(&path)?;
    let generated_schema = fixture.project().join("language.schema.json");
    let config = fs::read(fixture.project().join("kenvoy.toml"))?;
    let chain = fs::read(fixture.project().join("migrations/manifest.json"))?;
    for invalid in [
        None,
        Some(r#"{}"#),
        Some(r#"{"$schema":"./different.schema.json"}"#),
        Some(r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"English"}}"#),
        Some(
            r#"{"$schema":"./language.schema.json","title":{"cn":"中","zh-CN":"另一值","en":"English","ko":"한국어"}}"#,
        ),
        Some(
            r#"{"$schema":"./language.schema.json","title":{"cn":"中","en":"First","\u0065n":"Second","ko":"한국어"}}"#,
        ),
    ] {
        if let Some(invalid) = invalid {
            fs::write(&path, invalid)?;
        } else {
            fs::remove_file(&path)?;
        }
        for mode in ["dev", "run"] {
            fs::remove_file(&generated_schema)?;
            let diagnostic = rejected_start(&fixture, mode, &[])?;
            assert!(diagnostic.contains("translations.json"), "{diagnostic}");
            assert!(
                generated_schema.is_file(),
                "{mode} 应先重建 Schema 再校验译文"
            );
            if let Some(expected) = invalid {
                assert_eq!(fs::read_to_string(&path)?, expected);
            } else {
                assert!(diagnostic.contains("kenvoy i18n"), "{diagnostic}");
                assert!(!path.exists());
            }
            assert!(!fixture.project().join("data/app.sqlite").exists());
            assert!(!fixture.project().join("dist").exists());
            assert_eq!(fs::read(fixture.project().join("kenvoy.toml"))?, config);
            assert_eq!(
                fs::read(fixture.project().join("migrations/manifest.json"))?,
                chain
            );
        }
    }
    fs::write(&path, original)?;
    let mut server = fixture.start("dev")?;
    let mut session = Session::new(server.origin()).await?;
    let application = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(application["i18n"]["format_version"], 2);
    server.stop()?;
    assert_eq!(applied(&fixture)?.as_array().ok_or("缺少历史")?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn sourceless_run_requires_valid_matching_root_translations_and_recovers_without_node()
-> TestResult {
    let fixture = project("i18n-root-delivery")?;
    let built = build(&fixture)?;
    let manifest_bytes = fs::read(fixture.project().join("dist/manifest.json"))?;
    let private_path = version(&fixture, &built)?.join("i18n.json");
    let private_bytes = fs::read(&private_path)?;
    let path = fixture.project().join("translations.json");
    let original = fs::read(&path)?;
    let mut tampered: Value = serde_json::from_slice(&original)?;
    tampered["pages./customers.title"]["en"] = json!("Undelivered edit");
    archive_sources(&fixture)?;
    fs::remove_file(fixture.project().join("language.schema.json"))?;
    let invalid_documents = [
        None,
        Some(b"{}".to_vec()),
        Some(br#"{"$schema":"./wrong.schema.json"}"#.to_vec()),
        Some(br#"{"$schema":"./language.schema.json"}"#.to_vec()),
        Some(serde_json::to_vec(&tampered)?),
    ];
    for content in invalid_documents {
        if let Some(content) = &content {
            fs::write(&path, content)?;
        } else {
            fs::remove_file(&path)?;
        }
        rejected_start(&fixture, "run", &[])?;
        assert!(!fixture.project().join("data/app.sqlite").exists());
        assert_eq!(
            fs::read(fixture.project().join("dist/manifest.json"))?,
            manifest_bytes
        );
        assert_eq!(fs::read(&private_path)?, private_bytes);
        if let Some(content) = content {
            assert_eq!(fs::read(&path)?, content);
        } else {
            assert!(!path.exists());
        }
    }
    // 代码与别名只是输入拼写；相同规范化消息不应被误认为改过的产物。
    let mut canonical: Value = serde_json::from_slice(&original)?;
    for (key, value) in canonical.as_object_mut().ok_or("译文不是对象")? {
        if key == "$schema" {
            continue;
        }
        let row = value.as_object_mut().ok_or("词条不是语言矩阵")?;
        for (alias, code) in [("cn", "zh-CN"), ("en", "en-US"), ("ko", "ko-KR")] {
            if let Some(value) = row.remove(alias) {
                row.insert(code.into(), value);
            }
        }
    }
    let restored = serde_json::to_vec_pretty(&canonical)?;
    fs::write(&path, &restored)?;
    let mut server =
        fixture.start_with("run", &[], Duration::from_secs(45), &no_node(&fixture)?)?;
    let mut session = Session::new(server.origin()).await?;
    let application = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(
        application["i18n"]["messages"]["en-US"]["pages./customers.title"],
        "Customer registration"
    );
    server.stop()?;
    assert_eq!(manifest(&fixture)?["build_id"], built["build_id"]);
    assert_eq!(fs::read(path)?, restored);
    assert!(fixture.project().join("language.schema.json").is_file());
    Ok(())
}

#[tokio::test]
async fn invalid_translations_cannot_apply_pending_migrations_or_modify_existing_database()
-> TestResult {
    let fixture = project("i18n-pending-migration")?;
    let mut server = fixture.start("run")?;
    let mut session = Session::new(server.origin()).await?;
    let application = session.get("/_kenvoy/application", StatusCode::OK).await?;
    let form = form(&application)?;
    let created = session
        .post(
            "/api/customers",
            json!({"name":"迁移前保留","phone":"13400000000","age":37}),
            Some(&form),
            StatusCode::CREATED,
        )
        .await?;
    let id = created["data"]["id"].as_i64().ok_or("缺少记录 ID")?;
    server.stop()?;
    let source_path = fixture.project().join("src/app.tsx");
    let source = fs::read_to_string(&source_path)?;
    assert!(source.contains("</resource>"));
    fs::write(
        source_path,
        source.replacen("</resource>", "<text name='nickname' /></resource>", 1),
    )?;
    let generated = command(
        &fixture,
        &["migrate", "generate", "--description", "语言校验先于迁移"],
    )?;
    require_success(&generated)?;
    let generated: Value = serde_json::from_slice(&generated.stdout)?;
    let checksum = generated["confirmation_checksum"]
        .as_str()
        .ok_or("缺少迁移确认值")?;
    require_success(&command(
        &fixture,
        &["migrate", "seal", "--confirm", checksum],
    )?)?;
    let status = command(&fixture, &["migrate", "status"])?;
    require_success(&status)?;
    let status: Value = serde_json::from_slice(&status.stdout)?;
    assert_eq!(status["pending"].as_array().ok_or("缺少待执行链")?.len(), 1);
    let extra = status["confirmation_checksum"]
        .as_str()
        .map(|checksum| vec!["--confirm", checksum])
        .unwrap_or_default();
    let before_schema = schema(&fixture)?;
    let before_history = applied(&fixture)?;
    let database_path = fixture.project().join("data/app.sqlite");
    let database_bytes = fs::read(&database_path)?;
    let translations_path = fixture.project().join("translations.json");
    let original = fs::read(&translations_path)?;
    fs::write(
        &translations_path,
        r#"{"$schema":"./language.schema.json","title":{}}"#,
    )?;
    for mode in ["dev", "run"] {
        let diagnostic = rejected_start(&fixture, mode, &extra)?;
        assert!(diagnostic.contains("translations.json"), "{diagnostic}");
        assert_eq!(fs::read(&database_path)?, database_bytes);
        assert_eq!(schema(&fixture)?, before_schema);
        assert_eq!(
            stored(&fixture, id)?,
            ("迁移前保留".into(), "13400000000".into(), 37)
        );
    }
    fs::write(translations_path, original)?;
    assert_eq!(applied(&fixture)?, before_history);
    let mut recovered =
        fixture.start_with("run", &extra, Duration::from_secs(45), &BTreeMap::new())?;
    recovered.stop()?;
    assert_eq!(applied(&fixture)?.as_array().ok_or("缺少历史")?.len(), 2);
    assert_eq!(
        read_db(&fixture)?.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('customers') WHERE name='nickname'",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        1
    );
    assert_eq!(
        stored(&fixture, id)?,
        ("迁移前保留".into(), "13400000000".into(), 37)
    );
    Ok(())
}
