use std::{fs, path::Path};

use super::common::{Temporary, TestResult, compile, fixture, stage};
use crate::infrastructure::client_build::{
    ClientBuildError, ClientBuildOptions, build,
    command::{install_project_dependencies, run},
    digest,
    sources::LOCK,
};
use kengine::AnalysisSession;
use serde_json::json;

#[tokio::test]
async fn embedded_runtime_exports_without_node_and_rejects_existing_output() -> TestResult {
    let (_temporary, project) =
        fixture("export default <page path=\"/customers\"><text name=\"name\" required/></page>;")?;
    let (app, program) = compile(&project, &mut AnalysisSession::new())?;
    let destination = stage(&project, "embedded")?;
    let first = build(
        &project,
        &program,
        &app,
        &destination,
        ClientBuildOptions::default(),
    )
    .await?;
    assert!(first.files.contains_key("index.html"));
    assert_eq!(
        first
            .dependencies
            .get("framework:react")
            .map(String::as_str),
        Some("19.3.1")
    );
    assert_eq!(
        first.dependencies.get("framework:npm_lock_sha256"),
        Some(&digest(LOCK))
    );
    assert!(!project.root.join(".kenvoy/client-toolchain").exists());
    assert!(
        build(
            &project,
            &program,
            &app,
            &destination,
            ClientBuildOptions::default()
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn changed_analysis_and_nonregistry_install_fail_before_spawning_tools() -> TestResult {
    let (_temporary, project) = fixture(
        "import {Card} from './Card'; export default <page path=\"/card\"><Card/></page>;",
    )?;
    fs::write(
        project.root.join("src/Card.tsx"),
        "export function Card(){return <div/>;}",
    )?;
    let (app, program) = compile(&project, &mut AnalysisSession::new())?;
    fs::write(
        project.root.join("src/Card.tsx"),
        "export function Card(){return <div>changed</div>;}",
    )?;
    let result = build(
        &project,
        &program,
        &app,
        &stage(&project, "changed")?,
        ClientBuildOptions::default(),
    )
    .await;
    assert!(
        matches!(result,Err(ClientBuildError::Invalid(message)) if message.contains("源码已改变"))
    );
    assert!(!project.root.join(".kenvoy/client-toolchain").exists());
    for value in [
        "file:../private",
        "git+https://example.invalid/package",
        "workspace:*",
        "https://example.invalid/package.tgz",
    ] {
        fs::write(
            project.root.join("package.json"),
            serde_json::to_vec(&json!({"dependencies":{"example":value}}))?,
        )?;
        assert!(matches!(
            install_project_dependencies(&project, Path::new("missing-node")).await,
            Err(ClientBuildError::Invalid(_))
        ));
        assert!(!project.root.join("package-lock.json").exists());
    }
    Ok(())
}

#[tokio::test]
async fn process_output_is_bounded_and_timeout_reaps_direct_child() -> TestResult {
    let temporary = Temporary::new()?;
    let output = run(
        "node",
        &[
            "-e".into(),
            "process.stdout.write('x'.repeat(300000));process.stderr.write('y'.repeat(300000));"
                .into(),
        ],
        temporary.path(),
        "输出上限测试",
        5,
    )
    .await?;
    assert_eq!(output.len(), 256 * 1024);
    let result = run(
        "node",
        &["-e".into(), "setTimeout(()=>{},60000);".into()],
        temporary.path(),
        "期限测试",
        1,
    )
    .await;
    assert!(matches!(result, Err(ClientBuildError::Timeout { .. })));
    Ok(())
}
