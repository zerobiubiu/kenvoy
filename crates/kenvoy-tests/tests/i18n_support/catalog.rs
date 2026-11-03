//! 业务数据、正式迁移链与词典构建相互独立；无源码部署验证产物完整性。

use std::{fs, time::Duration};

use kenvoy_tests::{Fixture, TestResult, require_success};
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::support::{
    Session, account, applied, archive_sources, build, command, form, manifest, no_node, project,
    read_db, schema, stored, version,
};

fn edit_english(fixture: &Fixture, title: &str) -> TestResult {
    let path = fixture.project().join("locales/en-US.json");
    let mut dictionary: Value = serde_json::from_slice(&fs::read(&path)?)?;
    dictionary["pages./customers.title"] = json!(title);
    let bytes = serde_json::to_vec_pretty(&dictionary)?;
    fs::write(path, &bytes)?;
    Ok(())
}

#[tokio::test]
async fn catalog_changes_replace_build_without_schema_migration_and_deploy_without_sources()
-> TestResult {
    let fixture = project("i18n-artifact")?;
    account(&fixture, "init", "administrator")?;
    let mut server = fixture.start("run")?;
    let mut session = Session::new(server.origin()).await?;
    let model = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(
        model["i18n"]["messages"]["en-US"]["pages./customers.title"],
        "Customer registration"
    );
    let form_id = form(&model)?;
    let created = session
        .post(
            "/api/customers",
            json!({"name":"业务原值 customer","phone":"13800000000","age":38}),
            Some(&form_id),
            StatusCode::CREATED,
        )
        .await?;
    let id = created["data"]["id"].as_i64().ok_or("记录缺少 ID")?;
    server.stop()?;
    let first = manifest(&fixture)?;
    assert_eq!(first["format_version"], 3);
    assert_eq!(first["input_hash_version"], "sha256-input-v3");
    assert_eq!(first["build_hash_version"], "sha256-build-v3");
    let first_schema = schema(&fixture)?;
    let first_history = applied(&fixture)?;
    assert_eq!(first_history.as_array().ok_or("缺少已应用链")?.len(), 1);
    let chain = fs::read(fixture.project().join("migrations/manifest.json"))?;
    let record = stored(&fixture, id)?;
    edit_english(&fixture, "Revised customer registration")?;
    let second = build(&fixture)?;
    assert_ne!(second["build_id"], first["build_id"]);
    assert_ne!(second["input_fingerprint"], first["input_fingerprint"]);
    assert_ne!(
        second["files"]["i18n.json"]["sha256"],
        first["files"]["i18n.json"]["sha256"]
    );
    assert_eq!(
        second["i18n_config_hash"], first["i18n_config_hash"],
        "改译文不改配置身份"
    );
    assert_eq!(second["schema_hash_version"], first["schema_hash_version"]);
    assert_eq!(schema(&fixture)?, first_schema);
    assert_eq!(applied(&fixture)?, first_history);
    assert_eq!(
        fs::read(fixture.project().join("migrations/manifest.json"))?,
        chain
    );
    assert_eq!(stored(&fixture, id)?, record);
    let artifact: Value =
        serde_json::from_slice(&fs::read(version(&fixture, &second)?.join("i18n.json"))?)?;
    assert_eq!(artifact["format_version"], 1);
    assert_eq!(
        artifact["messages"]["en-US"]["pages./customers.title"],
        "Revised customer registration"
    );
    archive_sources(&fixture)?;
    for directory in ["src", "locales", "migrations", ".kenvoy"] {
        assert!(!fixture.project().join(directory).exists());
    }
    let mut deployed =
        fixture.start_with("run", &[], Duration::from_secs(45), &no_node(&fixture)?)?;
    let mut session = Session::new(deployed.origin()).await?;
    let model = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(
        model["i18n"]["messages"]["en-US"]["pages./customers.title"],
        "Revised customer registration"
    );
    assert_eq!(
        model["i18n"]["messages"]["zh-CN"]["resources.customers.fields.name.label"],
        "客户姓名"
    );
    session.login("administrator").await?;
    let restored = session
        .get(&format!("/api/customers/{id}"), StatusCode::OK)
        .await?;
    assert_eq!(restored["data"]["name"], "业务原值 customer");
    assert_eq!(restored["data"]["age"], 38);
    session.logout().await?;
    let form = form(&model)?;
    session
        .post(
            "/api/customers",
            json!({"name":"无源码新增","phone":"13900000000","age":40}),
            Some(&form),
            StatusCode::CREATED,
        )
        .await?;
    deployed.stop()?;
    assert_eq!(manifest(&fixture)?["build_id"], second["build_id"]);
    assert_eq!(schema(&fixture)?, first_schema);
    assert_eq!(applied(&fixture)?, first_history);
    assert_eq!(stored(&fixture, id)?, record);
    assert_eq!(
        read_db(&fixture)?.query_row("SELECT COUNT(*) FROM customers", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    let config_path = fixture.project().join("kenvoy.toml");
    let original_config = fs::read_to_string(&config_path)?;
    let changed_config =
        original_config.replace("default_locale = \"zh-CN\"", "default_locale = \"en-US\"");
    assert_ne!(changed_config, original_config, "夹具必须实际改变默认语言");
    fs::write(&config_path, changed_config)?;
    let rejected =
        fixture.command_with(&["--project", "app", "run"], "", Duration::from_secs(10))?;
    assert!(
        !rejected.status.success(),
        "不能用新语言配置冒充已有无源码产物"
    );
    assert!(String::from_utf8(rejected.stderr)?.contains("i18n 配置"));
    assert_eq!(manifest(&fixture)?["build_id"], second["build_id"]);
    assert_eq!(schema(&fixture)?, first_schema);
    assert_eq!(stored(&fixture, id)?, record);
    Ok(())
}

#[tokio::test]
async fn missing_or_modified_private_catalog_is_rejected_before_database_creation() -> TestResult {
    let fixture = project("i18n-corrupt")?;
    let built = build(&fixture)?;
    let path = version(&fixture, &built)?.join("i18n.json");
    let original = fs::read(&path)?;
    let manifest_bytes = fs::read(fixture.project().join("dist/manifest.json"))?;
    archive_sources(&fixture)?;
    fs::remove_file(&path)?;
    for content in [
        None,
        Some(
            json!({"format_version":1,"default_locale":"zh-CN","locales":["zh-CN","en-US"],"messages":{"zh-CN":{},"en-US":{"app.name":"Tampered catalog"}}}),
        ),
    ] {
        if let Some(content) = content {
            fs::write(&path, serde_json::to_vec(&content)?)?;
        }
        let rejected =
            fixture.command_with(&["--project", "app", "run"], "", Duration::from_secs(10))?;
        assert!(!rejected.status.success(), "无源码运行必须拒绝损坏词典");
        let diagnostic = String::from_utf8(rejected.stderr)?;
        assert!(
            diagnostic.contains("产物文件清单、大小或 SHA-256 不匹配"),
            "{diagnostic}"
        );
        assert!(!fixture.project().join("data/app.sqlite").exists());
        assert_eq!(
            fs::read(fixture.project().join("dist/manifest.json"))?,
            manifest_bytes
        );
    }
    fs::write(&path, original)?;
    let mut server =
        fixture.start_with("run", &[], Duration::from_secs(45), &no_node(&fixture)?)?;
    let mut session = Session::new(server.origin()).await?;
    let application = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(
        application["i18n"]["messages"]["en-US"]["app.name"],
        "Customer management"
    );
    server.stop()?;
    Ok(())
}

#[tokio::test]
async fn legacy_format_without_i18n_stays_chinese_on_new_cli_without_rewriting_config() -> TestResult
{
    let fixture = project("i18n-legacy")?;
    let config = "[project]\nname='app'\nversion='0.1.0'\nmin_cli_version='0.2.1'\nformat_version='0.2.0'\n\n[server]\nport=0\n";
    fs::write(fixture.project().join("kenvoy.toml"), config)?;
    // 旧项目未声明 i18n；邻近文件既不启用语言也不能阻止旧应用运行。
    fs::write(
        fixture.project().join("locales/en-US.json"),
        "not a declared catalog",
    )?;
    require_success(&command(&fixture, &["check"])?)?;
    let inspected = command(&fixture, &["inspect"])?;
    require_success(&inspected)?;
    let ir: Value = serde_json::from_slice(&inspected.stdout)?;
    assert_eq!(ir["format_version"], 1);
    assert_eq!(ir["name"], "客户管理系统");
    let built = build(&fixture)?;
    assert_eq!(built["project_format_version"], "0.2.0");
    assert_eq!(built["min_cli_version"], "0.2.1");
    assert_eq!(built["framework_version"], "0.3.0");
    let mut server = fixture.start("run")?;
    let mut session = Session::new(server.origin()).await?;
    let model = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(model["format_version"], 2);
    assert_eq!(model["name"], "客户管理系统");
    assert_eq!(model["pages"][0]["title"], "客户登记");
    assert_eq!(model["resources"][0]["fields"][0]["label"], "客户姓名");
    assert_eq!(model["i18n"]["default_locale"], "zh-CN");
    assert_eq!(model["i18n"]["locales"], json!(["zh-CN"]));
    assert_eq!(model["i18n"]["messages"]["zh-CN"], json!({}));
    let denied = session
        .get("/api/customers", StatusCode::UNAUTHORIZED)
        .await?;
    assert_eq!(denied["error"]["message"], "请先登录");
    assert_eq!(denied["error"]["message_key"], "KV5001");
    server.stop()?;
    assert_eq!(
        fs::read_to_string(fixture.project().join("kenvoy.toml"))?,
        config
    );
    Ok(())
}
