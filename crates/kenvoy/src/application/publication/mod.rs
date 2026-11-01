//! 已接受的代际发布由独立任务完成，数据库提交与模型替换共享同一读写屏障。

mod actions;
#[cfg(test)]
mod testing;

pub use actions::{ActionError, AuthorizedAction};

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::{
    sync::{RwLock, RwLockReadGuard, oneshot},
    task::JoinHandle,
};

use crate::{
    application::{execution::FunctionExecutor, resources::ResourceRepository},
    domain::model::{ApplicationIR, ExecutionEnvironment},
};

#[derive(Debug, thiserror::Error)]
pub enum PublicationError<E: std::error::Error + 'static> {
    #[error(transparent)]
    Repository(E),
    #[error("应用发布失败：{0}")]
    Publication(String),
}

pub struct Generation<E> {
    pub application: ApplicationIR,
    pub generation: u64,
    pub assets: Option<PathBuf>,
    pub javascript: Option<E>,
}

struct Shared<R, E> {
    model: RwLock<Generation<E>>,
    repository: R,
    publications: Mutex<Vec<JoinHandle<()>>>,
    closing: AtomicBool,
}

pub struct Publication<R, E> {
    shared: Arc<Shared<R, E>>,
}

impl<R, E> Clone for Publication<R, E> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<R: ResourceRepository, E: FunctionExecutor> Publication<R, E> {
    pub fn new(
        application: ApplicationIR,
        repository: R,
        generation: u64,
        assets: Option<PathBuf>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                model: RwLock::new(Generation {
                    application,
                    generation,
                    assets,
                    javascript: None,
                }),
                repository,
                publications: Mutex::new(vec![]),
                closing: AtomicBool::new(false),
            }),
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, Generation<E>> {
        self.shared.model.read().await
    }

    pub async fn with_executor(&self, javascript: E) -> Result<(), PublicationError<R::Error>> {
        let mut model = self.shared.model.write().await;
        validate_javascript::<E, R::Error>(&model.application, Some(&javascript))?;
        model.javascript = Some(javascript);
        Ok(())
    }

    pub async fn publish_prepared(
        &self,
        application: ApplicationIR,
        assets: Option<PathBuf>,
        javascript: Option<E>,
    ) -> Result<u64, PublicationError<R::Error>> {
        validate_javascript::<E, R::Error>(&application, javascript.as_ref())?;
        let (reply, response) = oneshot::channel();
        {
            let mut tasks = self
                .shared
                .publications
                .lock()
                .map_err(|_| PublicationError::Publication("发布队列锁损坏".into()))?;
            tasks.retain(|task| !task.is_finished());
            if self.shared.closing.load(Ordering::Acquire) || tasks.len() >= 64 {
                return Err(PublicationError::Publication(
                    "宿主正在关闭或发布队列已满".into(),
                ));
            }
            let shared = self.shared.clone();
            tasks.push(tokio::spawn(async move {
                let result = async {
                    let mut model = shared.model.write().await;
                    let generation = model.generation.checked_add(1).ok_or_else(|| {
                        PublicationError::Publication("应用代际编号已耗尽".into())
                    })?;
                    shared
                        .repository
                        .replace_model(application.clone())
                        .await
                        .map_err(PublicationError::Repository)?;
                    *model = Generation {
                        application,
                        generation,
                        assets,
                        javascript,
                    };
                    Ok(generation)
                }
                .await;
                let _ = reply.send(result);
            }));
        }
        response
            .await
            .map_err(|_| PublicationError::Publication("发布任务异常退出，状态需要核对".into()))?
    }

    pub async fn close(&self) -> Result<(), PublicationError<R::Error>> {
        let publications = {
            let mut publications = self
                .shared
                .publications
                .lock()
                .map_err(|_| PublicationError::Publication("应用发布队列锁损坏".into()))?;
            self.shared.closing.store(true, Ordering::Release);
            std::mem::take(&mut *publications)
        };
        for task in publications {
            task.await
                .map_err(|_| PublicationError::Publication("应用发布任务异常退出".into()))?;
        }
        Ok(())
    }
}

fn validate_javascript<E: FunctionExecutor, T: std::error::Error + 'static>(
    application: &ApplicationIR,
    javascript: Option<&E>,
) -> Result<(), PublicationError<T>> {
    if application.actions.iter().any(|action| {
        matches!(
            action.runtime,
            ExecutionEnvironment::Server | ExecutionEnvironment::Shared
        ) && !javascript.is_some_and(|runtime| runtime.has_function(&action.handler))
    }) {
        return Err(PublicationError::Publication(
            "服务端 Action 缺少对应代际的已编译函数".into(),
        ));
    }
    Ok(())
}
