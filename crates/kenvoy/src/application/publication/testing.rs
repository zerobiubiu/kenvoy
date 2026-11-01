use tokio::sync::RwLockWriteGuard;

use crate::application::{execution::FunctionExecutor, resources::ResourceRepository};

use super::{Generation, Publication};

impl<R: ResourceRepository, E: FunctionExecutor> Publication<R, E> {
    pub(crate) async fn write_for_test(&self) -> RwLockWriteGuard<'_, Generation<E>> {
        self.shared.model.write().await
    }

    pub(crate) fn has_publications_for_test(&self) -> Result<bool, &'static str> {
        self.shared
            .publications
            .lock()
            .map(|tasks| !tasks.is_empty())
            .map_err(|_| "发布锁损坏")
    }
}
