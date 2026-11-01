//! P03 的 HTTP 路由、真实服务与关闭期限。

use super::{
    ProbeResult,
    database::{DbCommand, DbWorker},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot};
use tower_http::{limit::RequestBodyLimitLayer, services::ServeDir};

struct AbortServerOnDrop(tokio::task::AbortHandle);

impl Drop for AbortServerOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

const SHELL: &str = "<!doctype html><div id=\"p03-probe-shell\"></div>";
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone)]
pub(super) struct ProbeState {
    pub(super) database: Arc<DbWorker>,
    // 只验证服务端身份门控；此探针开关不构成正式 Session 实现。
    pub(super) admin_token: Arc<str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCustomer {
    name: String,
    age: Option<i64>,
}

type ApiResult<E = (StatusCode, Json<Value>)> = Result<(StatusCode, Json<Value>), E>;

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message })))
}

async fn create_customer(
    State(state): State<ProbeState>,
    Json(input): Json<CreateCustomer>,
) -> ApiResult {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 100
        || input
            .age
            .is_some_and(|age| !(0..=MAX_SAFE_INTEGER).contains(&age))
    {
        return Err(api_error(StatusCode::UNPROCESSABLE_ENTITY, "字段校验失败"));
    }
    let (reply, receiver) = oneshot::channel();
    state
        .database
        .sender
        .try_send(DbCommand::Create {
            name: input.name,
            age: input.age,
            reply,
        })
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库队列不可用"))?;
    let record = receiver
        .await
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库任务中断"))?
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败"))?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn admin_count(State(state): State<ProbeState>, headers: HeaderMap) -> ApiResult {
    if headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        != Some(state.admin_token.as_ref())
    {
        return Err(api_error(StatusCode::UNAUTHORIZED, "需要管理员身份"));
    }
    let (reply, receiver) = oneshot::channel();
    state
        .database
        .sender
        .try_send(DbCommand::Count(reply))
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库队列不可用"))?;
    let count = receiver
        .await
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "数据库任务中断"))?
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败"))?;
    Ok((StatusCode::OK, Json(count)))
}

pub(super) struct ProbeServer {
    pub(super) address: SocketAddr,
    pub(super) shutdown: Option<oneshot::Sender<()>>,
    pub(super) task: Option<tokio::task::JoinHandle<io::Result<()>>>,
    pub(super) database: Arc<DbWorker>,
}

impl ProbeServer {
    pub(super) async fn start(directory: &Path, state: ProbeState) -> ProbeResult<Self> {
        let router = Router::new()
            .route("/customers", get(|| async { Html(SHELL) }))
            .route("/api/customers", post(create_customer).get(admin_count))
            .route("/admin/customers", get(admin_count))
            .nest_service("/assets", ServeDir::new(directory.join("client/assets")))
            .fallback(|| async { api_error(StatusCode::NOT_FOUND, "探针路由不存在") })
            .with_state(state.clone())
            .layer(RequestBodyLimitLayer::new(512));
        Self::serve(router, state.database).await
    }

    async fn serve(router: Router, database: Arc<DbWorker>) -> ProbeResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = receiver.await;
                })
                .await
        });
        Ok(Self {
            address,
            shutdown: Some(shutdown),
            task: Some(task),
            database,
        })
    }

    pub(super) async fn finish(self) -> ProbeResult {
        self.finish_with_deadline(Duration::from_secs(5)).await
    }

    pub(super) async fn finish_with_deadline(mut self, grace: Duration) -> ProbeResult {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(mut task) = self.task.take() {
            let _abort_on_drop = AbortServerOnDrop(task.abort_handle());
            match tokio::time::timeout(grace, &mut task).await {
                Ok(result) => result??,
                Err(_) => {
                    task.abort();
                    let reaped = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
                    let outcome = if reaped.is_ok() {
                        "已中止并回收服务任务"
                    } else {
                        "已请求中止但任务回收超过1秒"
                    };
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("Axum 关闭超过{grace:?}，{outcome}"),
                    )
                    .into());
                }
            }
        }
        self.database.stop();
        Ok(())
    }
}

impl Drop for ProbeServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.database.stop();
    }
}
