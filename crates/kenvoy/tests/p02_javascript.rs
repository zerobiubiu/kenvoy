//! 阶段 00 P02 技术探针；这些辅助类型不是 Kenvoy 的正式执行 API。

#[path = "support/p02_javascript.rs"]
mod support;

use rquickjs::{AsyncContext, AsyncRuntime, Context, Function, Module, Promise, Runtime, Value};
use serde_json::json;
use std::{
    collections::BTreeMap,
    error::Error,
    io,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use support::{
    AllowlistResolver, InterruptChildGuard, MemoryLoader, fixture_modules, install_loader,
};

type ProbeResult<T = (), E = Box<dyn Error>> = Result<T, E>;
type Denials = Arc<Mutex<Vec<String>>>;

const ROOT: &str = "/project/server";
const ENTRY: &str = "/project/server/main.js";
const MATH_TS: &str = include_str!("fixtures/p02-javascript/math.ts");
const MAIN_TS: &str = include_str!("fixtures/p02-javascript/main.ts");

#[test]
fn typescript_cross_module_and_json_round_trip() -> ProbeResult {
    let modules = fixture_modules()?;
    assert!(!modules[ENTRY].contains("interface Input"));
    assert!(!modules[&format!("{ROOT}/math.js")].contains("<T>"));
    let source = modules[ENTRY].clone();
    let runtime = Runtime::new()?;
    install_loader(
        &runtime,
        modules,
        Denials::default(),
        Arc::new(AtomicUsize::new(0)),
    );
    let context = Context::full(&runtime)?;
    let result = context.with(|ctx| -> rquickjs::Result<String> {
        let (module, initialized) = Module::declare(ctx.clone(), ENTRY, source)?.eval()?;
        initialized.finish::<()>()?;
        let calculate: Function = module.get("calculate")?;
        let input = ctx.json_parse(json!({"name": "张三", "amount": 12000}).to_string())?;
        let result: Value = calculate.call((input,))?;
        let stringify: Function = ctx.eval("(value) => JSON.stringify(value)")?;
        stringify.call((result,))
    })?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result)?,
        json!({"name":"张三","total":9600})
    );
    Ok(())
}

#[tokio::test]
async fn async_module_function_and_promise_bridge() -> ProbeResult {
    let modules = fixture_modules()?;
    let source = modules[ENTRY].clone();
    let runtime = AsyncRuntime::new()?;
    runtime
        .set_loader(
            AllowlistResolver {
                modules: modules.keys().cloned().collect(),
                denied: Denials::default(),
            },
            MemoryLoader {
                modules,
                dropped: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await;
    let context = AsyncContext::full(&runtime).await?;
    let amount = context
        .async_with(async |ctx| -> rquickjs::Result<f64> {
            let (module, initialized) = Module::declare(ctx.clone(), ENTRY, source)?.eval()?;
            initialized.into_future::<()>().await?;
            let calculate: Function = module.get("calculateAsync")?;
            let input = ctx.json_parse(json!({"name":"李四","amount":500}).to_string())?;
            let promise: Promise = calculate.call((input,))?;
            promise.into_future::<f64>().await
        })
        .await?;
    assert_eq!(amount, 450.0);
    Ok(())
}

#[tokio::test]
async fn unresolved_promise_has_host_deadline() -> ProbeResult {
    let runtime = AsyncRuntime::new()?;
    let context = AsyncContext::full(&runtime).await?;
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_millis(50),
        context.async_with(async |ctx| -> rquickjs::Result<()> {
            let pending: Promise = ctx.eval("new Promise(() => {})")?;
            pending.into_future::<()>().await
        }),
    )
    .await;
    assert!(
        result.is_err(),
        "永不完成的 Promise 必须由宿主 deadline 终止等待"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    drop(context);
    drop(runtime);
    let fresh = Runtime::new()?;
    let fresh_context = Context::full(&fresh)?;
    assert_eq!(fresh_context.with(|ctx| ctx.eval::<i32, _>("20 + 22"))?, 42);
    Ok(())
}

#[test]
fn synchronous_infinite_loop_is_interrupted() -> ProbeResult {
    const CHILD_MARKER: &str = "KENVOY_P02_INTERRUPT_CHILD";
    let mode = std::env::var_os(CHILD_MARKER);
    if mode.as_deref() == Some(std::ffi::OsStr::new("no-interrupt")) {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        println!("P02 无引擎interrupt的子进程由父进程独立终止");
        context.with(|ctx| ctx.eval::<(), _>("while (true) {}"))?;
        return Err("未配置 interrupt 的无限循环不应自行返回".into());
    }
    if mode.as_deref() != Some(std::ffi::OsStr::new("1")) {
        let child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "synchronous_infinite_loop_is_interrupted",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1")
            .spawn()?;
        let mut child = InterruptChildGuard(child);
        let Some(status) = child.wait_until(Instant::now() + Duration::from_secs(5))? else {
            child.stop()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "P02 引擎中断探针超过宿主5秒期限，已终止并回收独立子进程",
            )
            .into());
        };
        assert!(status.success(), "独立子进程中的引擎中断探针失败：{status}");
        let uncooperative = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "synchronous_infinite_loop_is_interrupted",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "no-interrupt")
            .spawn()?;
        let mut uncooperative = InterruptChildGuard(uncooperative);
        assert!(
            uncooperative
                .wait_until(Instant::now() + Duration::from_millis(250))?
                .is_none(),
            "未配置引擎中断的子进程应保持运行直到宿主期限"
        );
        uncooperative.stop()?;
        assert!(
            uncooperative.0.try_wait()?.is_some(),
            "宿主必须回收不合作子进程"
        );
        println!("P02 同一测试exe的独立子进程中断验证通过；父进程具有5秒看门狗");
        return Ok(());
    }
    let runtime = Runtime::new()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let observed = interrupted.clone();
    let deadline = Instant::now() + Duration::from_millis(50);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        if Instant::now() >= deadline {
            observed.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    })));
    let context = Context::full(&runtime)?;
    let started = Instant::now();
    context.with(|ctx| {
        assert!(ctx.eval::<(), _>("while (true) {}").is_err());
        let _ = ctx.catch();
    });
    assert!(interrupted.load(Ordering::SeqCst));
    assert!(started.elapsed() < Duration::from_secs(3));
    runtime.set_interrupt_handler(None);
    assert_eq!(context.with(|ctx| ctx.eval::<i32, _>("6 * 7"))?, 42);
    Ok(())
}

#[test]
fn native_allocator_memory_limit_raises_out_of_memory() -> ProbeResult {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    runtime.set_memory_limit(2 * 1024 * 1024);
    let interrupted = Arc::new(AtomicBool::new(false));
    let observed = interrupted.clone();
    let deadline = Instant::now() + Duration::from_secs(3);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let expired = Instant::now() >= deadline;
        if expired {
            observed.store(true, Ordering::SeqCst);
        }
        expired
    })));
    let message = context.with(|ctx| {
        // 单次请求超过限额，保留创建错误对象所需空间；完全耗尽时内核可能抛出 null。
        let result = ctx.eval::<(), _>("new ArrayBuffer(16 * 1024 * 1024)");
        assert!(result.is_err(), "实际分配必须达到 Runtime 内存限额");
        let exception = ctx.catch();
        exception
            .as_exception()
            .and_then(|error| error.message())
            .unwrap_or_default()
    });
    assert!(
        !interrupted.load(Ordering::SeqCst),
        "不能以超时冒充 OOM 验证"
    );
    assert!(message.contains("out of memory"), "实际异常：{message}");
    runtime.set_memory_limit(0);
    runtime.set_interrupt_handler(None);
    runtime.run_gc();
    assert_eq!(context.with(|ctx| ctx.eval::<i32, _>("40 + 2"))?, 42);
    Ok(())
}

#[test]
fn exception_is_caught_and_context_remains_usable() -> ProbeResult {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    context.with(|ctx| {
        assert!(
            ctx.eval::<(), _>("throw new Error('P02 独立异常')")
                .is_err()
        );
        let caught = ctx.catch();
        let message = caught.as_exception().and_then(|error| error.message());
        assert_eq!(message.as_deref(), Some("P02 独立异常"));
    });
    assert_eq!(context.with(|ctx| ctx.eval::<i32, _>("42"))?, 42);
    Ok(())
}

#[test]
fn traversal_node_and_unregistered_modules_are_denied() -> ProbeResult {
    for name in [
        "../../private.js",
        "node:fs",
        "./missing.js",
        "https://example.test/a.js",
        "./%2e%2e/private.js",
        "./nested\\secret.js",
    ] {
        let runtime = Runtime::new()?;
        let denials = Denials::default();
        install_loader(
            &runtime,
            fixture_modules()?,
            denials.clone(),
            Arc::new(AtomicUsize::new(0)),
        );
        let context = Context::full(&runtime)?;
        let specifier = serde_json::to_string(name)?;
        context.with(|ctx| {
            let result = Module::evaluate(
                ctx.clone(),
                ENTRY,
                format!("import secret from {specifier};"),
            );
            assert!(result.is_err(), "必须拒绝模块 {name}");
            let _ = ctx.catch();
        });
        assert_eq!(*denials.lock().map_err(|_| "拒绝日志锁失效")?, vec![name]);
    }
    Ok(())
}

#[test]
fn context_has_no_implicit_filesystem_network_or_node_host() -> ProbeResult {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    let capabilities = context.with(|ctx| ctx.eval::<String, _>(
        "JSON.stringify([typeof process, typeof require, typeof fetch, typeof Deno, typeof std, typeof os])",
    ))?;
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&capabilities)?,
        vec!["undefined"; 6]
    );
    Ok(())
}

#[test]
fn contexts_and_loader_are_released_with_runtime() -> ProbeResult {
    let dropped = Arc::new(AtomicUsize::new(0));
    for _ in 0..20 {
        let runtime = Runtime::new()?;
        let weak = runtime.weak();
        install_loader(
            &runtime,
            BTreeMap::new(),
            Denials::default(),
            dropped.clone(),
        );
        let context = Context::full(&runtime)?;
        assert_eq!(context.with(|ctx| ctx.eval::<i32, _>("21 * 2"))?, 42);
        drop(context);
        runtime.run_gc();
        drop(runtime);
        assert!(weak.try_ref().is_none(), "Runtime 不得被遗留上下文持有");
    }
    assert_eq!(dropped.load(Ordering::SeqCst), 20);
    Ok(())
}
