//! 词典监听与代际发布通过真实 HTTP 和持久化数据观察。

use std::{fs, time::Duration};

use kenvoy_tests::TestResult;
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::support::{Session, applied, form, manifest, project, schema, stored};

async fn development(
    session: &mut Session,
    predicate: impl Fn(&Value) -> bool,
) -> TestResult<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let value = session.get("/_kenvoy/development", StatusCode::OK).await?;
        if predicate(&value) {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("词典监听未进入预期状态：{value}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn dev_catalog_reload_publishes_generation_and_keeps_invalid_candidate_inactive() -> TestResult
{
    let fixture = project("i18n-reload")?;
    let mut server = fixture.start("dev")?;
    let mut session = Session::new(server.origin()).await?;
    let initial = session.get("/_kenvoy/application", StatusCode::OK).await?;
    let generation = initial["generation"].as_u64().ok_or("缺少代际")?;
    let form = form(&initial)?;
    let created = session
        .post(
            "/api/customers",
            json!({"name":"监听数据","phone":"13600000000","age":35}),
            Some(&form),
            StatusCode::CREATED,
        )
        .await?;
    let id = created["data"]["id"].as_i64().ok_or("缺少记录 ID")?;
    let before_schema = schema(&fixture)?;
    let before_history = applied(&fixture)?;
    let before_manifest = fs::read(fixture.project().join("dist/manifest.json"))?;
    let path = fixture.project().join("locales/en-US.json");
    fs::write(&path, r#"{"app.name":"First","app.name":"Duplicate"}"#)?;
    let rejected = development(&mut session, |value| value["state"] == "error").await?;
    assert_eq!(rejected["generation"], generation);
    assert_eq!(
        session.get("/_kenvoy/application", StatusCode::OK).await?,
        initial
    );
    assert_eq!(
        fs::read(fixture.project().join("dist/manifest.json"))?,
        before_manifest
    );
    // 修复同一份真实文件，而不是跳过被拒绝候选或重启服务。
    fs::write(
        &path,
        serde_json::to_vec(
            &json!({"app.name":"Live application","pages./customers.title":"Live customer registration"}),
        )?,
    )?;
    let ready = development(&mut session, |value| {
        value["state"] == "ready"
            && value["generation"]
                .as_u64()
                .is_some_and(|value| value > generation)
    })
    .await?;
    let updated = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(updated["generation"], ready["generation"]);
    assert_eq!(
        updated["i18n"]["messages"]["en-US"]["pages./customers.title"],
        "Live customer registration"
    );
    assert_ne!(
        manifest(&fixture)?["build_id"],
        serde_json::from_slice::<Value>(&before_manifest)?["build_id"]
    );
    server.stop()?;
    assert_eq!(schema(&fixture)?, before_schema);
    assert_eq!(applied(&fixture)?, before_history);
    assert_eq!(
        stored(&fixture, id)?,
        ("监听数据".into(), "13600000000".into(), 35)
    );
    Ok(())
}
