use kengine::AnalysisSession;

use super::{
    build,
    error::{self, Result},
    lifecycle,
};
use crate::{
    application::{
        execution::{Limits, PreparedBundle},
        identity::Auth,
    },
    infrastructure::{
        artifacts as artifact, identity::SystemCredentials, javascript,
        persistence::sqlite as database, project::Project,
    },
    interfaces::{cli, http},
};

pub(super) async fn run_server(
    project: &Project,
    output: artifact::Loaded,
    development: Option<(AnalysisSession, build::Candidate)>,
) -> Result<()> {
    let bundle: PreparedBundle = serde_json::from_slice(&output.server_modules)?;
    javascript::validate(&bundle)?;
    let application = output.application;
    let assets = output.client;
    let listener =
        tokio::net::TcpListener::bind((project.config.server.host, project.config.server.port))
            .await
            .map_err(|source| error::io("HTTP 监听端口", source))?;
    let bound = listener
        .local_addr()
        .map_err(|source| error::io("HTTP 监听地址", source))?;
    let origin = project.config.server.resolve_origin(bound.port())?;
    let database = database::Database::open(project.database.clone(), application.clone()).await?;
    let mut javascript_runtime = None;
    let mut development_runtime = None;
    let serving: Result<()> = async {
        let auth = Auth::new(database.clone(), SystemCredentials).await?;
        let runtime = javascript::JavascriptRuntime::start(
            std::sync::Arc::new(bundle),
            Limits::default(),
            project.config.runtime.trust,
        )?;
        javascript_runtime = Some(runtime.clone());
        let server = http::HttpRuntime::new(
            application,
            database.clone(),
            auth,
            origin.clone(),
            Some(assets),
        )
        .await?
        .with_javascript(runtime.clone())
        .await?;
        if let Some((session, candidate)) = development {
            development_runtime = Some(
                Box::pin(lifecycle::Development::start(
                    project.clone(),
                    session,
                    candidate,
                    server.clone(),
                    runtime,
                    origin.clone(),
                ))
                .await?,
            );
        }
        let stop = development_runtime
            .as_ref()
            .map(lifecycle::Development::stop_signal);
        cli::server_ready(&origin);
        server
            .serve(listener, async move {
                if let Err(error) = tokio::signal::ctrl_c().await {
                    tracing::error!(target:"kenvoy::lifecycle", %error,"监听退出信号失败");
                }
                if let Some(stop) = stop {
                    let _ = stop.send(true);
                }
            })
            .await
            .map_err(|source| error::io("HTTP 服务", source))?;
        Ok(())
    }
    .await;
    let dev_shutdown = match development_runtime {
        Some(runtime) => runtime.shutdown().await,
        None => Ok(()),
    };
    let js_shutdown = match javascript_runtime {
        Some(runtime) => runtime.shutdown().await,
        None => Ok(()),
    };
    let shutdown = database.shutdown().await;
    serving?;
    dev_shutdown?;
    js_shutdown?;
    shutdown?;
    tracing::info!(target:"kenvoy::lifecycle","HTTP、开发任务、JavaScript 与数据库已关闭");
    Ok(())
}
