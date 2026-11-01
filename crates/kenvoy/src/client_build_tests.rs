use super::*;
use kengine::AnalysisSession;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

struct Temporary {
    path: PathBuf,
}
impl Temporary {
    fn new() -> std::io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-client-test-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if let (Ok(path), Ok(root)) = (
            self.path.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) && path.starts_with(root)
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("kenvoy-client-test-"))
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn fixture(source: &str) -> std::result::Result<(Temporary, Project), Box<dyn std::error::Error>> {
    let temporary = Temporary::new()?;
    fs::create_dir(temporary.path().join("src"))?;
    fs::write(
        temporary.path().join("kenvoy.toml"),
        "[project]\nname=\"client-test\"\n",
    )?;
    fs::write(temporary.path().join("src/app.tsx"), source)?;
    let project = Project::load(temporary.path())?;
    Ok((temporary, project))
}

fn stage(project: &Project, name: &str) -> Result<PathBuf> {
    directory(
        &project.root,
        &project.root.join(".kenvoy").join(name).join("client"),
    )
}

#[tokio::test]
async fn embedded_runtime_exports_without_node_and_rejects_existing_output() -> TestResult {
    let (_temporary, project) =
        fixture("export default <page path=\"/customers\"><text name=\"name\" required/></page>;")?;
    let (app, program) = crate::compile(&project, &mut AnalysisSession::new())?;
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
    let (app, program) = crate::compile(&project, &mut AnalysisSession::new())?;
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

#[tokio::test]
#[ignore = "需要 Node/npm 并实际准备锁定构建缓存及显式用户 npm 依赖；单独执行"]
async fn real_custom_build_installs_only_explicit_user_dependencies_and_reuses_toolchain()
-> TestResult {
    let (_temporary, project) = fixture(
        r#"import {Counter} from './barrel'; import {discount} from './discount.server'; const prefix='客户'; export default <app name="扩展"><action name="discount" runtime="server" permission="anonymous" handler={discount}/><page path="/counter"><Counter initial={2} format={(value:number)=>prefix+value}/></page></app>;"#,
    )?;
    fs::write(
        project.root.join("src/Counter.tsx"),
        r#"import {useState} from 'react'; import clsx from 'clsx'; import {useAction} from '@kenvoy/client'; export function Counter({initial,format}:{initial:number,format:(value:number)=>string}){const [value,setValue]=useState(initial);const discount=useAction('discount');return <button className={clsx('custom-counter',{active:value>0})} onClick={()=>{setValue(value+1);void discount([value]);}}>{format(value)}</button>;}"#,
    )?;
    fs::write(
        project.root.join("src/barrel.ts"),
        "import './counter.css'; export {Counter} from './Counter';",
    )?;
    fs::write(
        project.root.join("src/counter.css"),
        ".custom-counter{border-color:rgb(12,34,56)}",
    )?;
    fs::write(
        project.root.join("src/discount.server.ts"),
        "export function discount(amount:number){const PRIVATE_SERVER_SENTINEL='never-browser';return amount>100?0.8:0.9;}",
    )?;
    let package = br#"{"private":true,"dependencies":{"clsx":"2.1.1"},"scripts":{"postinstall":"node -e \"require('fs').writeFileSync('unsafe-install-ran','bad')\""}}"#;
    fs::write(project.root.join("package.json"), package)?;
    let (app, program) = crate::compile(&project, &mut AnalysisSession::new())?;
    let failed = build(
        &project,
        &program,
        &app,
        &stage(&project, "without-install")?,
        ClientBuildOptions::default(),
    )
    .await;
    assert!(failed.is_err(), "没有显式安装不能隐式写入用户依赖");
    assert!(!project.root.join("package-lock.json").exists());
    assert!(!project.root.join("node_modules").exists());
    let first = build(
        &project,
        &program,
        &app,
        &stage(&project, "with-install")?,
        ClientBuildOptions { install_deps: true },
    )
    .await?;
    assert_eq!(
        first.dependencies.get("project:clsx").map(String::as_str),
        Some("2.1.1")
    );
    assert!(!project.root.join("unsafe-install-ran").exists());
    assert_eq!(fs::read(project.root.join("package.json"))?, package);
    assert!(project.root.join("package-lock.json").is_file());
    let text = first
        .files
        .keys()
        .filter(|name| name.ends_with(".js") || name.ends_with(".css"))
        .map(|name| fs::read_to_string(first.client_dir.join(name)))
        .collect::<std::io::Result<Vec<_>>>()?
        .join("\n");
    assert!(text.contains("custom-counter"));
    assert!(!text.contains("PRIVATE_SERVER_SENTINEL"));
    let lock_before = fs::read(project.root.join("package-lock.json"))?;
    let second = build(
        &project,
        &program,
        &app,
        &stage(&project, "reuse")?,
        ClientBuildOptions::default(),
    )
    .await?;
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(
        fs::read(project.root.join("package-lock.json"))?,
        lock_before
    );
    let mut development = development::DevClient::start(
        &project,
        &program,
        &app,
        ClientBuildOptions::default(),
        "http://127.0.0.1:3000",
    )
    .await?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let endpoint = format!("http://{}/_kenvoy/vite/", development.endpoint());
    assert_eq!(
        client
            .get(format!("{endpoint}index.html"))
            .send()
            .await?
            .status()
            .as_u16(),
        403
    );
    let denied_websocket = tokio_tungstenite::connect_async(format!(
        "ws://{}/_kenvoy/vite/hmr",
        development.endpoint()
    ))
    .await;
    assert!(
        matches!(denied_websocket,Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status().as_u16()==403),
        "无内部凭据的 WebSocket 必须明确拒绝"
    );
    let html_response = client
        .get(format!("{endpoint}index.html"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?;
    assert!(html_response.status().is_success());
    let html = html_response.text().await?;
    assert!(html.contains("/_kenvoy/vite/preamble.js"), "{html}");
    assert!(
        !html.contains("<script type=\"module\">"),
        "CSP 不允许内联脚本：{html}"
    );
    let preamble = client
        .get(format!("{endpoint}preamble.js"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?
        .text()
        .await?;
    assert!(preamble.contains("injectIntoGlobalHook"), "{preamble}");
    let mut dependency_urls = BTreeSet::new();
    for entry in ["src/main.tsx", "src/App.tsx"] {
        let response = client
            .get(format!("{endpoint}{entry}"))
            .header("x-kenvoy-vite-secret", development.proxy_secret())
            .send()
            .await?;
        assert!(
            response.status().is_success(),
            "框架模块 {entry} 必须可以加载"
        );
        let source = response.text().await?;
        dependency_urls.extend(
            source
                .split('"')
                .filter(|part| {
                    part.starts_with("/_kenvoy/vite/") && part.contains("/vite-cache/deps/")
                })
                .map(str::to_owned),
        );
    }
    assert!(
        dependency_urls
            .iter()
            .any(|url| url.contains("react-dom_client")),
        "必须覆盖真实初始依赖优化请求"
    );
    let requests = dependency_urls.into_iter().map(|path| {
        client
            .get(format!("http://{}{path}", development.endpoint()))
            .header("x-kenvoy-vite-secret", development.proxy_secret())
            .send()
    });
    for response in futures_util::future::join_all(requests).await {
        let response = response?;
        assert!(
            response.status().is_success(),
            "初始依赖优化不应被抢先404：{} {}",
            response.status(),
            response.url()
        );
    }
    let original = project
        .entry
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    assert_eq!(
        client
            .get(format!("{endpoint}@fs/{original}?raw"))
            .header("x-kenvoy-vite-secret", development.proxy_secret())
            .send()
            .await?
            .status()
            .as_u16(),
        404
    );
    let registry_response = client
        .get(format!("{endpoint}@id/__x00__kenvoy-registry"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?;
    assert!(registry_response.status().is_success());
    let registry = registry_response.text().await?;
    let barrel = registry
        .split('"')
        .find(|part| part.starts_with("/_kenvoy/vite/") && part.contains("barrel.ts"))
        .ok_or_else(|| format!("注册表缺少镜像barrel导入：{registry}"))?;
    let upstream = format!("http://{}", development.endpoint());
    let barrel_source = client
        .get(format!("{upstream}{barrel}"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?
        .text()
        .await?;
    let counter = barrel_source
        .split('"')
        .find(|part| part.starts_with("/_kenvoy/vite/") && part.contains("Counter.tsx"))
        .ok_or_else(|| format!("barrel缺少镜像组件导入：{barrel_source}"))?;
    let source = client
        .get(format!("{upstream}{counter}"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?
        .text()
        .await?;
    assert!(source.contains("custom-counter"));
    let counter_path = project.root.join("src/Counter.tsx");
    let next_source =
        fs::read_to_string(&counter_path)?.replace("custom-counter", "updated-counter");
    fs::write(&counter_path, next_source)?;
    let (next_app, next_program) = crate::compile(&project, &mut AnalysisSession::new())?;
    let update = development
        .prepare_update(&project, &next_program, &next_app)
        .await?;
    // 候选准备完成仍然不能改变浏览器读取的旧镜像。
    let before = client
        .get(format!("{upstream}{counter}?t=before"))
        .header("x-kenvoy-vite-secret", development.proxy_secret())
        .send()
        .await?
        .text()
        .await?;
    assert!(before.contains("custom-counter"));
    development.commit_update(update, false).await?;
    let mut changed = false;
    for attempt in 0..50 {
        let after = client
            .get(format!("{upstream}{counter}?t={attempt}"))
            .header("x-kenvoy-vite-secret", development.proxy_secret())
            .send()
            .await?
            .text()
            .await?;
        if after.contains("updated-counter") {
            changed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(changed, "已提交镜像必须触发 Vite 模块更新");
    development.shutdown().await?;
    assert!(
        client
            .get(format!("{endpoint}index.html"))
            .send()
            .await
            .is_err()
    );
    Ok(())
}
