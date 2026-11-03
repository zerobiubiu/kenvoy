use std::{future::IntoFuture, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router, middleware,
    routing::{get, post},
};
use tokio::{net::TcpListener, sync::Notify};

use super::{
    HttpRuntime, actions, development, handlers, middleware::protect, session, static_files,
    uploads,
};

impl HttpRuntime {
    pub fn router(&self) -> Router {
        Router::new()
            .route("/_kenvoy/session", get(session::session))
            .route("/_kenvoy/login", post(session::login))
            .route("/_kenvoy/logout", post(session::logout))
            .route("/_kenvoy/application", get(handlers::application))
            .route("/_kenvoy/development", get(development::status))
            .route("/_kenvoy/vite/hmr", get(development::websocket))
            .route("/_kenvoy/vite/{*path}", get(development::assets))
            .route(
                "/_kenvoy/uploads/{resource}/{field}",
                post(uploads::upload).layer(axum::extract::DefaultBodyLimit::disable()),
            )
            .route(
                "/_kenvoy/files/{id}",
                get(uploads::read).delete(uploads::remove),
            )
            .route("/api/actions/{name}", post(actions::action))
            .route("/api/{resource}/options/{field}", post(handlers::options))
            .route(
                "/api/{resource}",
                get(handlers::list).post(handlers::create),
            )
            .route(
                "/api/{resource}/{id}",
                get(handlers::read)
                    .patch(handlers::update)
                    .delete(handlers::delete),
            )
            .fallback(static_files::fallback)
            .method_not_allowed_fallback(static_files::method_not_allowed)
            .layer(middleware::from_fn_with_state(self.shared.clone(), protect))
            .with_state(self.shared.clone())
    }

    /// 宿主负责在 HTTP 优雅退出完成后关闭数据库工作线程。
    /// 停止接收后最多等待连接排空；超时继续关闭宿主资源，不无限挂起。
    pub async fn serve<F>(&self, listener: TcpListener, shutdown: F) -> std::io::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        const DRAIN_LIMIT: Duration = Duration::from_secs(10);
        let shared = self.shared.clone();
        let draining = Arc::new(Notify::new());
        let started = draining.clone();
        let server = axum::serve(
            listener,
            self.router()
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            shutdown.await;
            if let Err(error) = development::stop(&shared).await {
                tracing::error!(target:"kenvoy::http",%error,"开发连接关闭失败");
            }
            started.notify_one();
        });
        let deadline = async move {
            draining.notified().await;
            tokio::time::sleep(DRAIN_LIMIT).await;
        };
        let mut server = Box::pin(server.into_future());
        let result = tokio::select! {
            result = server.as_mut() => result,
            _ = deadline => {
                tracing::warn!(
                    target:"kenvoy::http",
                    seconds = DRAIN_LIMIT.as_secs(),
                    "HTTP 优雅退出等待超时，仍有连接未结束；继续关闭自有资源"
                );
                Ok(())
            }
        };
        drop(server);
        development::stop(&self.shared)
            .await
            .map_err(|_| std::io::Error::other("开发连接未能退出"))?;
        self.shared
            .model
            .close()
            .await
            .map_err(|error| match error {
                crate::application::publication::PublicationError::Publication(message) => {
                    std::io::Error::other(message)
                }
                crate::application::publication::PublicationError::Repository(error) => {
                    std::io::Error::other(error)
                }
            })?;
        result
    }
}
