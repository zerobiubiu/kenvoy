use super::*;
use crate::javascript::{BUNDLE_VERSION, FunctionTarget, PreparedModule};
use serde_json::json;
use std::collections::BTreeMap;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn bundle(code: &str, names: &[&str]) -> Arc<PreparedBundle> {
    Arc::new(PreparedBundle {
        format_version: BUNDLE_VERSION,
        modules: BTreeMap::from([(
            "m_1".into(),
            PreparedModule {
                code: code.into(),
                imports: BTreeMap::new(),
            },
        )]),
        functions: names
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    FunctionTarget {
                        module: "m_1".into(),
                        export: name.to_string(),
                    },
                )
            })
            .collect(),
    })
}

#[tokio::test]
async fn json_sync_async_and_fresh_contexts() -> TestResult {
    let runtime = JavascriptRuntime::start(
        bundle(
            "let counter=0; export function sync(v){counter++;return {value:v,count:counter};} export async function asynchronous(v){await Promise.resolve();return [v,null,true,42];} export function capabilities(){return [typeof process,typeof globalThis['require'],typeof fetch,typeof std,typeof os];}",
            &["sync", "asynchronous", "capabilities"],
        ),
        Limits::default(),
        Trust::Project,
    )?;
    for _ in 0..2 {
        assert_eq!(
            runtime.call("sync", vec![json!("张三")]).await?,
            json!({"value":"张三","count":1})
        );
    }
    assert_eq!(
        runtime.call("asynchronous", vec![json!(7)]).await?,
        json!([7, null, true, 42])
    );
    assert_eq!(
        runtime.call("capabilities", vec![]).await?,
        json!([
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined"
        ])
    );
    runtime.shutdown().await?;
    assert!(matches!(
        runtime.call("sync", vec![]).await,
        Err(JsError::Closed)
    ));
    Ok(())
}

#[tokio::test]
async fn json_rejects_silent_loss_cycles_accessors_and_tampered_stringify() -> TestResult {
    for expression in [
        "undefined",
        "NaN",
        "Infinity",
        "9007199254740992",
        "1n",
        "(()=>{})",
        "({a:undefined})",
        "[1,,3]",
        "new Date()",
        "new Map()",
        "({get x(){return 1}})",
        "(()=>{const a={};a.x=a;return a})()",
        "({[Symbol('x')]:1})",
        "(()=>{Object.prototype.value=1;return {get x(){return 1}}})()",
    ] {
        let code =
            format!("export function run(){{ JSON.stringify=()=> '{{}}'; return {expression};}}");
        let runtime =
            JavascriptRuntime::start(bundle(&code, &["run"]), Limits::default(), Trust::Project)?;
        assert!(
            matches!(
                runtime.call("run", vec![]).await,
                Err(JsError::InvalidOutput)
            ),
            "{expression}"
        );
        runtime.shutdown().await?;
    }
    Ok(())
}

#[tokio::test]
async fn pending_promises_exceptions_and_generations_are_isolated() -> TestResult {
    let runtime = JavascriptRuntime::start(
        bundle(
            "export function pending(){return new Promise(()=>{});} export function fail(){throw new Error('private-secret');} export function good(){return 1;}",
            &["pending", "fail", "good"],
        ),
        Limits {
            timeout: Duration::from_millis(100),
            ..Limits::default()
        },
        Trust::Project,
    )?;
    assert!(matches!(
        runtime.call("pending", vec![]).await,
        Err(JsError::Timeout)
    ));
    assert!(matches!(
        runtime.call("fail", vec![]).await,
        Err(JsError::Execution)
    ));
    assert_eq!(runtime.call("good", vec![]).await?, json!(1));
    let updated = runtime.with_bundle(bundle("export function good(){return 2;}", &["good"]))?;
    assert_eq!(updated.call("good", vec![]).await?, json!(2));
    assert_eq!(runtime.call("good", vec![]).await?, json!(1));
    runtime.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn limits_registry_and_untrusted_are_enforced() -> TestResult {
    let source = bundle(
        "export function run(value){return value;} export function large(){return 'x'.repeat(2000);}",
        &["run", "large"],
    );
    assert!(matches!(
        JavascriptRuntime::start(source.clone(), Limits::default(), Trust::Untrusted),
        Err(JsError::Untrusted)
    ));
    let runtime = JavascriptRuntime::start(
        source,
        Limits {
            json_bytes: 1024,
            arguments: 2,
            ..Limits::default()
        },
        Trust::Project,
    )?;
    assert!(matches!(
        runtime.call("unknown", vec![]).await,
        Err(JsError::UnknownFunction)
    ));
    assert!(matches!(
        runtime
            .call("run", vec![json!(1), json!(2), json!(3)])
            .await,
        Err(JsError::InvalidInput)
    ));
    assert!(matches!(
        runtime.call("run", vec![json!("x".repeat(2000))]).await,
        Err(JsError::InvalidInput)
    ));
    assert!(matches!(
        runtime.call("large", vec![]).await,
        Err(JsError::InvalidOutput)
    ));
    assert!(matches!(
        runtime
            .call("run", vec![json!({"nested":[9_007_199_254_740_993_u64]})])
            .await,
        Err(JsError::InvalidInput)
    ));
    let mut nested = Json::Null;
    for _ in 0..66 {
        nested = json!([nested]);
    }
    assert!(matches!(
        runtime.call("run", vec![nested]).await,
        Err(JsError::InvalidInput)
    ));
    runtime.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn bounded_queue_cancel_and_shutdown_release_pending_promises() -> TestResult {
    use std::{
        future::{Future, poll_fn},
        pin::Pin,
        task::Poll,
    };

    type Call<'a> = Pin<Box<dyn Future<Output = Result<Json, JsError>> + 'a>>;
    async fn admitted<'a>(
        runtime: &'a JavascriptRuntime,
        handler: &'static str,
    ) -> Result<Call<'a>, JsError> {
        loop {
            let mut call: Call<'a> = Box::pin(runtime.call(handler, vec![]));
            match poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await {
                Poll::Pending => return Ok(call),
                // Busy 明确表示未入队；等待真实容量，不以固定 sleep 猜测线程进度。
                Poll::Ready(Err(JsError::Busy)) => tokio::task::yield_now().await,
                Poll::Ready(Err(error)) => return Err(error),
                Poll::Ready(Ok(_)) => panic!("排队场景中的调用不应提前完成"),
            }
        }
    }

    let runtime = JavascriptRuntime::start(
        bundle(
            "export function pending(){return new Promise(()=>{});} export function good(){return 42;}",
            &["pending", "good"],
        ),
        Limits {
            timeout: Duration::from_secs(5),
            queue: 1,
            ..Limits::default()
        },
        Trust::Project,
    )?;
    let first =
        tokio::time::timeout(Duration::from_secs(2), admitted(&runtime, "pending")).await??;
    // 容量为1；第二次已入队证明第一个已被工作线程取走，而且当前队列确实已满。
    let second =
        tokio::time::timeout(Duration::from_secs(2), admitted(&runtime, "pending")).await??;
    assert!(matches!(
        runtime.call("good", vec![]).await,
        Err(JsError::Busy)
    ));
    drop(first);
    drop(second);
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                match runtime.call("good", vec![]).await {
                    Err(JsError::Busy) => tokio::task::yield_now().await,
                    result => break result,
                }
            }
        })
        .await??,
        json!(42)
    );
    let pending =
        tokio::time::timeout(Duration::from_secs(2), admitted(&runtime, "pending")).await??;
    let queued = tokio::time::timeout(Duration::from_secs(2), admitted(&runtime, "good")).await??;
    tokio::time::timeout(Duration::from_millis(500), runtime.shutdown()).await??;
    assert!(matches!(pending.await, Err(JsError::Closed)));
    assert!(matches!(queued.await, Err(JsError::Closed)));
    Ok(())
}

#[test]
fn cpu_loop_and_memory_limits_have_independent_process_watchdog() -> TestResult {
    const MARKER: &str = "KENVOY_JS_RUNTIME_LIMIT_CHILD";
    if std::env::var_os(MARKER).is_some() {
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        return executor.block_on(async {
            let runtime=JavascriptRuntime::start(bundle("export function loop(){while(true){}} export async function jobs(){while(true){await Promise.resolve();}} export function memory(){const values=[];while(true){values.push(new Array(50000).fill('abcdef'));}} export function good(){return 42;}",&["loop","jobs","memory","good"]),Limits{timeout:Duration::from_millis(200),memory_bytes:2*1024*1024,..Limits::default()},Trust::Project)?;
            assert!(matches!(runtime.call("loop",vec![]).await,Err(JsError::Timeout)));
            assert!(matches!(runtime.call("jobs",vec![]).await,Err(JsError::Timeout)));
            assert!(matches!(runtime.call("memory",vec![]).await,Err(JsError::Memory|JsError::Execution)));
            assert_eq!(runtime.call("good",vec![]).await?,json!(42));
            runtime.shutdown().await?;Ok(())
        });
    }
    let mut child=std::process::Command::new(std::env::current_exe()?).args(["--exact","javascript::runtime::tests::cpu_loop_and_memory_limits_have_independent_process_watchdog","--nocapture"]).env(MARKER,"1").spawn()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait()? {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err("JavaScript 限额子进程超过宿主期限，已回收".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
