use kengine::AnalysisSession;

use super::{
    build,
    error::{self, Error, Result},
    lifecycle,
    startup::Startup,
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
    shutdown: super::signals::Shutdown,
    confirmation: Option<String>,
) -> Result<()> {
    project.check_compatibility()?;
    let bundle: PreparedBundle = serde_json::from_slice(&output.server_modules)?;
    javascript::validate(&bundle)?;
    // 数据库工作线程持有生命周期锁，直至全部请求处理完且连接关闭。
    let database = if project.snapshot {
        database::Database::open_snapshot(project.database.clone(), output.application.clone())
            .await?
    } else {
        database::Database::open_catalog(
            project.database.clone(),
            output.application.clone(),
            output.migrations.clone(),
            confirmation,
        )
        .await?
    };
    let migrated_state = if project.snapshot {
        "开发快照已建立，应用未就绪"
    } else {
        "数据库已迁移，应用未就绪"
    };
    let mut startup = match Startup::new(project, &output.manifest, &output.migrations) {
        Ok(startup) => startup,
        Err(error) => {
            if let Err(shutdown) = database.shutdown().await {
                tracing::error!(target:"kenvoy::lifecycle",%shutdown,"启动记录失败后数据库关闭异常");
            }
            return Err(Error::Activation {
                state: migrated_state,
                source: Box::new(error),
            });
        }
    };
    let mut ready = false;
    let mut javascript_runtime = None;
    let mut development_runtime = None;
    let serving: Result<()> = async {
        startup.record("migrated", None)?;
        // 占用端口等激活失败不回滚已经提交的迁移，也不自动恢复备份。
        let listener =
            tokio::net::TcpListener::bind((project.config.server.host, project.config.server.port))
                .await
                .map_err(|source| error::io("HTTP 监听端口", source))?;
        let bound = listener
            .local_addr()
            .map_err(|source| error::io("HTTP 监听地址", source))?;
        let origin = project.config.server.resolve_origin(bound.port())?;
        let auth = Auth::new(database.clone(), SystemCredentials).await?;
        let runtime = javascript::JavascriptRuntime::start(
            std::sync::Arc::new(bundle),
            Limits::default(),
            project.config.runtime.trust,
        )?;
        javascript_runtime = Some(runtime.clone());
        let server = http::HttpRuntime::new(
            output.application,
            database.clone(),
            auth,
            origin.clone(),
            Some(output.client),
            output.i18n,
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
        let requested = shutdown.wait_owned();
        // 就绪定义：迁移验证完成、运行态与开发任务初始化成功、监听端口已取得。
        startup.record("ready", None)?;
        ready = true;
        cli::server_ready(&origin);
        server
            .serve(listener, async move {
                requested.await;
                if let Some(stop) = stop {
                    let _ = stop.send(true);
                }
            })
            .await
            .map_err(|source| error::io("HTTP 服务", source))?;
        Ok(())
    }
    .await;
    if let Err(error) = &serving {
        let phase = if ready {
            "runtime_failed"
        } else {
            "startup_failed"
        };
        if let Err(record) = startup.record(phase, Some(&error.to_string())) {
            tracing::error!(target:"kenvoy::lifecycle",%record,"启动失败记录未能保存");
        }
    }
    const JS_SHUTDOWN_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);
    const DATABASE_SHUTDOWN_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);
    let dev_shutdown = match development_runtime {
        Some(runtime) => runtime.shutdown().await,
        None => Ok(()),
    };
    let js_shutdown = match javascript_runtime {
        Some(runtime) => match tokio::time::timeout(JS_SHUTDOWN_LIMIT, runtime.shutdown()).await {
            Ok(result) => result.map_err(Error::from),
            Err(_) => Err(Error::ShutdownTimeout("JavaScript 运行时")),
        },
        None => Ok(()),
    };
    let shutdown = match tokio::time::timeout(DATABASE_SHUTDOWN_LIMIT, database.shutdown()).await {
        Ok(result) => result.map_err(Error::from),
        Err(_) => Err(Error::ShutdownTimeout("SQLite 数据库")),
    };
    let closing = dev_shutdown.and(js_shutdown).and(shutdown);
    if let Err(error) = &closing {
        startup.record("shutdown_failed", Some(&error.to_string()))?;
    }
    if let Err(error) = serving {
        if !ready {
            return Err(Error::Activation {
                state: migrated_state,
                source: Box::new(error),
            });
        }
        return Err(error);
    }
    closing?;
    startup.record("stopped", None)?;
    tracing::info!(target:"kenvoy::lifecycle","HTTP、开发任务、JavaScript 与数据库已关闭");
    Ok(())
}
