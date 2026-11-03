use super::*;

#[tokio::test]
async fn server_actions_execute_compiled_esm_with_permissions_csrf_and_atomic_generations()
-> TestResult {
    use crate::{
        application::execution::{Limits, Trust},
        infrastructure::javascript::{self, JavascriptRuntime},
    };
    let mut f = Fixture::new("export default <page path='/actions'/>;").await?;
    fs::write(f.root.join("math.ts"), "export const factor:number=2;")?;
    fs::write(
        f.root.join("actions.server.ts"),
        "import {factor} from './math'; export async function sum(value:number){await Promise.resolve();return {result:value*factor};} export function secret(){return '仅管理员';} export function fail(){throw new Error('private-secret-path');} export function pending(){return new Promise(()=>{});}",
    )?;
    let source = "import {sum,secret,fail,pending} from './actions.server'; export default <app name='动作'><action name='sum' runtime='server' permission='anonymous' handler={sum}/><action name='secret' runtime='server' permission='admin' handler={secret}/><action name='fail' runtime='server' permission='anonymous' handler={fail}/><action name='pending' runtime='server' permission='anonymous' handler={pending}/><page path='/actions'/></app>;";
    fs::write(f.root.join("app.tsx"), source)?;
    let program = kengine::Engine::new().analyze_project(
        &f.root,
        kengine::AnalyzeOptions {
            entry_points: vec!["app.tsx".into()],
            ..Default::default()
        },
    )?;
    let app = crate::application::compilation::analyze(
        &program,
        &f.root.join("app.tsx").canonicalize()?,
        "测试",
        &crate::infrastructure::javascript::SwcClientCodeTransformer,
    )
    .application
    .ok_or("Action 分析失败")?;
    let bundle = javascript::prepare(&program, &app, Trust::Project)?;
    let js = JavascriptRuntime::start(
        Arc::new(bundle.clone()),
        Limits {
            timeout: Duration::from_millis(150),
            ..Limits::default()
        },
        Trust::Project,
    )?;
    f.database.replace_application(app.clone(), None).await?;
    f.runtime = HttpRuntime::new(
        app.clone(),
        f.database.clone(),
        f.auth.clone(),
        "http://127.0.0.1:3000".into(),
        None,
        Default::default(),
    )
    .await?
    .with_javascript(js.clone())
    .await?;
    let (cookie, csrf) = f.context().await?;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", csrf.as_str()),
        ("content-type", "application/json"),
    ];
    let response = f
        .request("POST", "/api/actions/sum", &headers, r#"{"args":[21]}"#)
        .await?;
    assert_eq!(
        response.0,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response.2)
    );
    let result: Value = serde_json::from_slice(&response.2)?;
    assert_eq!(result["data"], json!({"result":42}));
    let old_generation = result["generation"].as_u64().ok_or("缺少代际")?;
    assert_eq!(
        f.request("POST", "/api/actions/secret", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request("POST", "/api/actions/unknown", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("GET", "/api/actions/sum", &[], Body::empty())
            .await?
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    for body in [
        r#"{"args":[],"extra":1}"#,
        r#"{"args":[],"args":[]}"#,
        r#"{"args":[{"value":1,"value":2}]}"#,
        r#"{"args":[9007199254740993]}"#,
    ] {
        assert_eq!(
            f.request("POST", "/api/actions/sum", &headers, body)
                .await?
                .0,
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    assert_eq!(
        f.request("POST", "/api/actions/sum", &headers[..2], r#"{"args":[1]}"#)
            .await?
            .0,
        StatusCode::FORBIDDEN
    );
    let failed = f
        .request("POST", "/api/actions/fail", &headers, r#"{"args":[]}"#)
        .await?;
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!String::from_utf8_lossy(&failed.2).contains("private-secret"));
    assert_eq!(
        f.request("POST", "/api/actions/pending", &headers, r#"{"args":[]}"#)
            .await?
            .0,
        StatusCode::GATEWAY_TIMEOUT
    );
    let (admin_cookie, admin_csrf) = f.login().await?;
    let admin_headers = [
        ("cookie", admin_cookie.as_str()),
        ("origin", "http://127.0.0.1:3000"),
        ("x-kenvoy-csrf", admin_csrf.as_str()),
        ("content-type", "application/json"),
    ];
    assert_eq!(
        f.request(
            "POST",
            "/api/actions/secret",
            &admin_headers,
            r#"{"args":[]}"#
        )
        .await?
        .0,
        StatusCode::OK
    );
    let mut updated = bundle;
    for module in updated.modules.values_mut() {
        module.code = module.code.replace("factor = 2", "factor = 3");
    }
    let updated_js = js.with_bundle(Arc::new(updated))?;
    let next = f
        .runtime
        .publish_prepared(app.clone(), None, Some(updated_js), Default::default())
        .await?;
    assert_eq!(next, old_generation + 1);
    assert!(
        f.runtime
            .publish_prepared(app, None, None, Default::default())
            .await
            .is_err()
    );
    let response = f
        .request("POST", "/api/actions/sum", &headers, r#"{"args":[14]}"#)
        .await?;
    assert_eq!(response.0, StatusCode::OK);
    let result: Value = serde_json::from_slice(&response.2)?;
    assert_eq!(result["data"], json!({"result":42}));
    assert_eq!(result["generation"], next);
    let mut stale = headers.to_vec();
    let stale_generation = old_generation.to_string();
    stale.push(("x-kenvoy-generation", stale_generation.as_str()));
    assert_eq!(
        f.request("POST", "/api/actions/sum", &stale, r#"{"args":[1]}"#)
            .await?
            .0,
        StatusCode::CONFLICT
    );
    js.shutdown().await?;
    f.database.shutdown().await?;
    Ok(())
}
