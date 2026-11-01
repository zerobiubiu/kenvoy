mod engine;

use crate::application::execution::{FunctionExecutor, JsError, Limits, PreparedBundle, Trust};
use engine::{execute, validate_input};
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

impl FunctionExecutor for JavascriptRuntime {
    fn has_function(&self, handler: &str) -> bool {
        Self::has_function(self, handler)
    }

    async fn call(&self, handler: &str, args: Vec<Json>) -> Result<Json, JsError> {
        Self::call(self, handler, args).await
    }
}

#[cfg(test)]
mod tests;
