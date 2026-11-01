use super::{PreparedBundle, Trust};
use rquickjs::{
    AsyncContext, AsyncRuntime, Ctx, Function, Module, Value,
    function::Args,
    loader::{ImportAttributes, Loader, Resolver},
    module::Declared,
};
use serde_json::Value as Json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub json_bytes: usize,
    pub arguments: usize,
    pub queue: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1),
            memory_bytes: 32 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            json_bytes: 1024 * 1024,
            arguments: 32,
            queue: 32,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self, JsError> {
        if !(Duration::from_millis(10)..=Duration::from_secs(30)).contains(&self.timeout)
            || !(2 * 1024 * 1024..=128 * 1024 * 1024).contains(&self.memory_bytes)
            || !(64 * 1024..=1024 * 1024).contains(&self.stack_bytes)
            || !(1024..=1024 * 1024).contains(&self.json_bytes)
            || !(1..=32).contains(&self.arguments)
            || !(1..=64).contains(&self.queue)
        {
            return Err(JsError::Configuration);
        }
        Ok(self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JsError {
    #[error("KV6201：尚不支持在当前进程执行不可信 JavaScript")]
    Untrusted,
    #[error("KV6202：JavaScript 运行时限制配置无效")]
    Configuration,
    #[error("KV6203：JavaScript 函数不在已验证的注册表中")]
    UnknownFunction,
    #[error("KV6204：函数参数不符合 JSON 格式、数量或大小限制")]
    InvalidInput,
    #[error("KV6205：函数返回值不是受限 JSON 数据或超过大小限制")]
    InvalidOutput,
    #[error("KV6206：JavaScript 执行失败，异常细节未向调用方公开")]
    Execution,
    #[error("KV6207：JavaScript 执行超过时间限制")]
    Timeout,
    #[error("KV6208：JavaScript 内存不足")]
    Memory,
    #[error("KV6209：JavaScript 执行队列已满，此次调用未执行")]
    Busy,
    #[error("KV6210：JavaScript 运行时正在关闭，此次调用未执行")]
    Closed,
    #[error("KV6211：JavaScript 工作线程不可用")]
    WorkerLost,
    #[error(transparent)]
    Bundle(#[from] super::JsDiagnostic),
}

struct Job {
    bundle: Arc<PreparedBundle>,
    handler: String,
    args: Vec<Json>,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    reply: oneshot::Sender<Result<Json, JsError>>,
}
struct Worker {
    sender: SyncSender<Job>,
    closed: Arc<AtomicBool>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    limits: Limits,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
    }
}
#[derive(Clone)]
pub struct JavascriptRuntime {
    worker: Arc<Worker>,
    bundle: Arc<PreparedBundle>,
}
struct Cancel(Arc<AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl JavascriptRuntime {
    pub fn start(
        bundle: Arc<PreparedBundle>,
        limits: Limits,
        trust: Trust,
    ) -> Result<Self, JsError> {
        if trust != Trust::Project {
            return Err(JsError::Untrusted);
        }
        super::validate(&bundle)?;
        let limits = limits.validate()?;
        let (sender, receiver) = mpsc::sync_channel::<Job>(limits.queue);
        let closed = Arc::new(AtomicBool::new(false));
        let worker_closed = closed.clone();
        let thread=thread::Builder::new().name("kenvoy-javascript".into()).spawn(move||{
            let runtime=match tokio::runtime::Builder::new_current_thread().enable_time().build(){Ok(runtime)=>runtime,Err(_)=>return};
            loop {
                if worker_closed.load(Ordering::Acquire) {while let Ok(job)=receiver.try_recv(){let _=job.reply.send(Err(JsError::Closed));}break;}
                let job=match receiver.recv_timeout(Duration::from_millis(20)){Ok(job)=>job,Err(mpsc::RecvTimeoutError::Timeout)=>continue,Err(mpsc::RecvTimeoutError::Disconnected)=>break};
                if job.cancel.load(Ordering::Acquire) || job.reply.is_closed(){continue;}
                let started=Instant::now();
                let result=if started>=job.deadline {Err(JsError::Timeout)} else {runtime.block_on(execute(&job,limits,worker_closed.clone()))};
                tracing::debug!(target:"kenvoy::javascript",handler=%job.handler,duration_ms=started.elapsed().as_millis(),success=result.is_ok(),"受限函数执行结束");
                let _=job.reply.send(result);
            }
        }).map_err(|_|JsError::WorkerLost)?;
        Ok(Self {
            worker: Arc::new(Worker {
                sender,
                closed,
                thread: Mutex::new(Some(thread)),
                limits,
            }),
            bundle,
        })
    }

    /// 新旧代际分别捕获不可变模块包，复用有界线程；切换不替换正在执行的函数源码。
    pub fn with_bundle(&self, bundle: Arc<PreparedBundle>) -> Result<Self, JsError> {
        super::validate(&bundle)?;
        if self.worker.closed.load(Ordering::Acquire) {
            return Err(JsError::Closed);
        }
        Ok(Self {
            worker: self.worker.clone(),
            bundle,
        })
    }
    pub fn has_function(&self, handler: &str) -> bool {
        self.bundle.functions.contains_key(handler)
    }
    pub async fn call(&self, handler: &str, args: Vec<Json>) -> Result<Json, JsError> {
        if self.worker.closed.load(Ordering::Acquire) {
            return Err(JsError::Closed);
        }
        if !self.has_function(handler) {
            return Err(JsError::UnknownFunction);
        }
        if args.len() > self.worker.limits.arguments {
            return Err(JsError::InvalidInput);
        }
        let mut nodes = 0;
        for input in &args {
            validate_input(input, 0, &mut nodes)?;
        }
        if serde_json::to_vec(&args)
            .map_err(|_| JsError::InvalidInput)?
            .len()
            > self.worker.limits.json_bytes
        {
            return Err(JsError::InvalidInput);
        }
        let (reply, response) = oneshot::channel();
        let cancel = Cancel(Arc::new(AtomicBool::new(false)));
        let deadline = Instant::now() + self.worker.limits.timeout;
        let job = Job {
            bundle: self.bundle.clone(),
            handler: handler.into(),
            args,
            deadline,
            cancel: cancel.0.clone(),
            reply,
        };
        self.worker
            .sender
            .try_send(job)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => JsError::Busy,
                mpsc::TrySendError::Disconnected(_) => JsError::WorkerLost,
            })?;
        tokio::time::timeout(self.worker.limits.timeout, response)
            .await
            .map_err(|_| JsError::Timeout)?
            .map_err(|_| JsError::WorkerLost)?
    }
    pub async fn shutdown(&self) -> Result<(), JsError> {
        self.worker.closed.store(true, Ordering::Release);
        let handle = self
            .worker
            .thread
            .lock()
            .map_err(|_| JsError::WorkerLost)?
            .take();
        if let Some(handle) = handle {
            tokio::task::spawn_blocking(move || handle.join())
                .await
                .map_err(|_| JsError::WorkerLost)?
                .map_err(|_| JsError::WorkerLost)?;
        }
        Ok(())
    }
}

fn validate_input(value: &Json, depth: usize, nodes: &mut usize) -> Result<(), JsError> {
    *nodes += 1;
    if depth > 64 || *nodes > 100000 {
        return Err(JsError::InvalidInput);
    }
    match value {
        Json::Number(number) => {
            if number.as_i64().is_some_and(|value| {
                !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value)
            }) || number
                .as_u64()
                .is_some_and(|value| value > 9_007_199_254_740_991)
                || number.as_f64().is_some_and(|value| {
                    !value.is_finite()
                        || (value.fract() == 0.0 && value.abs() > 9_007_199_254_740_991.0)
                })
            {
                return Err(JsError::InvalidInput);
            }
        }
        Json::Array(values) => {
            for value in values {
                validate_input(value, depth + 1, nodes)?;
            }
        }
        Json::Object(values) => {
            for value in values.values() {
                validate_input(value, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct BundleResolver(Arc<PreparedBundle>);
impl Resolver for BundleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if attributes.is_none()
            && let Some(target) = self
                .0
                .modules
                .get(base)
                .and_then(|module| module.imports.get(name))
        {
            return Ok(target.clone());
        }
        Err(rquickjs::Error::new_resolving_message(
            base,
            name,
            "模块不在该函数的静态依赖白名单内",
        ))
    }
}
struct BundleLoader(Arc<PreparedBundle>);
impl Loader for BundleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if attributes.is_none()
            && let Some(module) = self.0.modules.get(name)
        {
            return Module::declare(ctx.clone(), name, module.code.as_bytes());
        }
        Err(rquickjs::Error::new_loading_message(name, "模块未注册"))
    }
}
fn quick_error(error: rquickjs::Error) -> JsError {
    if matches!(error, rquickjs::Error::Allocation) {
        JsError::Memory
    } else {
        JsError::Execution
    }
}

async fn execute(job: &Job, limits: Limits, closed: Arc<AtomicBool>) -> Result<Json, JsError> {
    let runtime = AsyncRuntime::new().map_err(quick_error)?;
    runtime.set_memory_limit(limits.memory_bytes).await;
    runtime.set_max_stack_size(limits.stack_bytes).await;
    let cancel = job.cancel.clone();
    let deadline = job.deadline;
    let interruption = closed.clone();
    runtime
        .set_interrupt_handler(Some(Box::new(move || {
            cancel.load(Ordering::Acquire)
                || interruption.load(Ordering::Acquire)
                || Instant::now() >= deadline
        })))
        .await;
    runtime
        .set_loader(
            BundleResolver(job.bundle.clone()),
            BundleLoader(job.bundle.clone()),
        )
        .await;
    let context = AsyncContext::full(&runtime).await.map_err(quick_error)?;
    let target = job
        .bundle
        .functions
        .get(&job.handler)
        .ok_or(JsError::UnknownFunction)?;
    let code = &job
        .bundle
        .modules
        .get(&target.module)
        .ok_or(JsError::UnknownFunction)?
        .code;
    let work = context.async_with(async |ctx| -> Result<String, JsError> {
        // 在用户模块初始化前捕获原生方法，用户修改 JSON/Object 原型不能绕过返回值检查。
        let serialize: Function = ctx.eval(STRICT_JSON).map_err(quick_error)?;
        let (module, ready) = Module::declare(ctx.clone(), target.module.as_str(), code.as_bytes())
            .map_err(quick_error)?
            .eval()
            .map_err(quick_error)?;
        ready.into_future::<()>().await.map_err(quick_error)?;
        let function: Function = module.get(target.export.as_str()).map_err(quick_error)?;
        let mut args = Args::new(ctx.clone(), job.args.len());
        for input in &job.args {
            args.push_arg(ctx.json_parse(input.to_string()).map_err(quick_error)?)
                .map_err(quick_error)?;
        }
        let result: Value = function.call_arg(args).map_err(quick_error)?;
        let result = match result.as_promise() {
            Some(promise) => promise
                .clone()
                .into_future::<Value>()
                .await
                .map_err(quick_error)?,
            None => result,
        };
        serialize
            .call((result, limits.json_bytes))
            .map_err(|_| JsError::InvalidOutput)
    });
    let cancelled = async {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if closed.load(Ordering::Acquire) || job.cancel.load(Ordering::Acquire) {
                return;
            }
        }
    };
    let result = tokio::select! {
        result=tokio::time::timeout(deadline.saturating_duration_since(Instant::now()),work)=>result,
        _=cancelled=>return Err(if closed.load(Ordering::Acquire){JsError::Closed}else{JsError::Timeout}),
    };
    if closed.load(Ordering::Acquire) {
        return Err(JsError::Closed);
    }
    if job.cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(JsError::Timeout);
    }
    let text = result.map_err(|_| JsError::Timeout)??;
    if text.len() > limits.json_bytes {
        return Err(JsError::InvalidOutput);
    }
    serde_json::from_str(&text).map_err(|_| JsError::InvalidOutput)
}

const STRICT_JSON: &str = r#"(() => {
 const own=Reflect.ownKeys, hasOwn=Object.hasOwn, descriptors=Object.getOwnPropertyDescriptors, proto=Object.getPrototypeOf,
 setProto=Object.setPrototypeOf, create=Object.create, array=Array.isArray, finite=Number.isFinite, integer=Number.isInteger, safe=Number.isSafeInteger,
 stringify=JSON.stringify, objectProto=Object.prototype, arrayProto=Array.prototype, ErrorType=Error;
 return (input,limit)=>{
  const visiting=[]; let nodes=0;
  const walk=(value,depth)=>{
   if(++nodes>100000||depth>64) throw new ErrorType('JSON 限额');
   if(value===null||typeof value==='boolean') return value;
   if(typeof value==='string'){if(value.length>limit)throw new ErrorType('JSON 长度');return value;}
   if(typeof value==='number'){if(!finite(value)||(integer(value)&&!safe(value)))throw new ErrorType('JSON 数值');return value;}
   if(typeof value!=='object')throw new ErrorType('JSON 类型');
   for(let i=0;i<visiting.length;i++)if(visiting[i]===value)throw new ErrorType('JSON 循环');
   const isArray=array(value),p=proto(value);
   if((isArray&&p!==arrayProto&&p!==null)||(!isArray&&p!==objectProto&&p!==null))throw new ErrorType('JSON 原型');
   const keys=own(value),d=descriptors(value),out=isArray?setProto([],null):create(null);
   visiting[visiting.length]=value;
   if(isArray){
    const length=d.length.value;if(keys.length!==length+1)throw new ErrorType('JSON 稀疏数组或附加属性');
    for(let i=0;i<length;i++){const item=d[i];if(!item||!hasOwn(item,'value'))throw new ErrorType('JSON 访问器');out[i]=walk(item.value,depth+1);}
   }else{for(let i=0;i<keys.length;i++){const key=keys[i],item=d[key];if(typeof key!=='string'||!hasOwn(item,'value')||!item.enumerable)throw new ErrorType('JSON 属性');out[key]=walk(item.value,depth+1);}}
   visiting.length--;return out;
  };
  const text=stringify(walk(input,0));if(text.length>limit)throw new ErrorType('JSON 长度');return text;
 };
})()"#;

#[cfg(test)]
mod tests;
