//! 每次调用的独立 QuickJS 上下文、白名单加载器与 JSON 边界。

use rquickjs::{
    AsyncContext, AsyncRuntime, Ctx, Function, Module, Value,
    function::Args,
    loader::{ImportAttributes, Loader, Resolver},
    module::Declared,
};
use serde_json::Value as Json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use super::Job;
use crate::application::execution::{JsError, Limits, PreparedBundle};
pub(super) fn validate_input(value: &Json, depth: usize, nodes: &mut usize) -> Result<(), JsError> {
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

pub(super) async fn execute(
    job: &Job,
    limits: Limits,
    closed: Arc<AtomicBool>,
) -> Result<Json, JsError> {
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
