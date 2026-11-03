//! 原生监听只唤醒有界协调队列；候选失败不发布模型、Schema、镜像或活动清单。
use super::{
    build::{self, Candidate},
    error::{Error, Result},
};
use crate::{
    application::execution::PreparedBundle,
    domain::model::ModuleReferenceKind,
    infrastructure::{
        artifacts as artifact,
        client_build::{ClientBuildOptions, development::DevClient},
        javascript::JavascriptRuntime,
        project::Project,
    },
    interfaces::http::{DevelopmentPhase, DevelopmentProxy, HttpRuntime},
};
use kengine::AnalysisSession;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

pub struct Development {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
}
impl Development {
    pub async fn start(
        project: Project,
        session: AnalysisSession,
        candidate: Candidate,
        http: HttpRuntime,
        javascript: JavascriptRuntime,
        origin: String,
    ) -> Result<Self> {
        let mut client = if custom(&candidate) {
            Some(
                Box::pin(DevClient::start(
                    &project,
                    &candidate.program,
                    &candidate.application,
                    ClientBuildOptions::default(),
                    &origin,
                ))
                .await?,
            )
        } else {
            None
        };
        if let Err(error) = http.enable_development(proxy(client.as_ref())).await {
            if let Some(client) = &mut client {
                let _ = client.shutdown().await;
            }
            return Err(error.into());
        }
        if let Err(error) = artifact::activate(&project, &candidate.output.manifest) {
            if let Some(client) = &mut client {
                let _ = client.shutdown().await;
            }
            return Err(error.into());
        }
        let (events, mut receive) = mpsc::channel(1);
        let root = project.root.clone();
        let output = project.out_dir.clone();
        let data = project
            .database
            .parent()
            .unwrap_or(&project.root)
            .to_owned();
        let persistent = project.root.join(
            project
                .config
                .database
                .path
                .parent()
                .unwrap_or(Path::new("data")),
        );
        let mut native: RecommendedWatcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event)
                    if !matches!(event.kind, EventKind::Access(_))
                        && event.paths.iter().any(|path| {
                            !path.starts_with(&persistent) && relevant(path, &root, &output, &data)
                        }) =>
                {
                    let _ = events.try_send(());
                }
                Err(_) => {
                    let _ = events.try_send(());
                }
                _ => {}
            })
            .map_err(|error| Error::InvalidConfig(format!("无法建立文件监听：{error}")))?;
        native
            .watch(&project.root, RecursiveMode::Recursive)
            .map_err(|error| Error::InvalidConfig(format!("无法监听项目：{error}")))?;
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let _native = native;
            let mut project = project;
            let mut session = session;
            let mut candidate = candidate;
            let mut observed = candidate.output.manifest.input_fingerprint.clone();
            let mut rejected = false;
            // 关闭启动时建立监听之前的变更窗口。
            let mut initial = true;
            loop {
                if !initial {
                    tokio::select! {_ = stopped.changed()=>break,event=receive.recv()=>if event.is_none(){break;}}
                }
                initial = false;
                tokio::select! {_=stopped.changed()=>break,_=tokio::time::sleep(Duration::from_millis(180))=>{}}
                while receive.try_recv().is_ok() {}
                let mut committed = false;
                let changed = async {
                    let mut current = Project::load(&project.root)?;
                    current.check_compatibility()?;
                    crate::infrastructure::project::i18n::prepare(&current, false)?;
                    if project.snapshot {
                        if current.config.database.path != project.config.database.path {
                            return Err(Error::InvalidConfig(
                                "正式数据库路径变更需要停止 dev；快照不会隐式接入正式数据库".into(),
                            ));
                        }
                        current.database = project.database.clone();
                        current.snapshot = true;
                    }
                    if current.database != project.database
                        || current.out_dir != project.out_dir
                        || current.config.server.host != project.config.server.host
                        || current.config.server.port != project.config.server.port
                        || current.config.server.public_origin
                            != project.config.server.public_origin
                    {
                        return Err(Error::InvalidConfig("数据库、输出目录或监听地址变更需要重新启动 dev；当前服务继续使用原配置".into()));
                    }
                    let fingerprint = artifact::input_fingerprint(&current)?;
                    if fingerprint == observed {
                        if rejected && fingerprint == candidate.output.manifest.input_fingerprint {
                            http.set_development_status(DevelopmentPhase::Ready, None, false)
                                .await?;
                            rejected = false;
                        }
                        return Ok(());
                    }
                    observed = fingerprint;
                    http.set_development_status(DevelopmentPhase::Building, None, false)
                        .await?;
                    let next = Box::pin(build::candidate(&current, &mut session, false)).await?;
                    if serde_json::to_value(&next.output.migrations)?
                        != serde_json::to_value(&candidate.output.migrations)?
                    {
                        return Err(Error::InvalidConfig(
                            "正式迁移链发生变化，请停止 dev 后重新启动；运行期间不执行迁移，旧服务继续使用原迁移位置".into(),
                        ));
                    }
                    let structural = next.application != candidate.application;
                    let bundle: PreparedBundle =
                        serde_json::from_slice(&next.output.server_modules)?;
                    let next_js = javascript.with_bundle(Arc::new(bundle))?;
                    let needs_client = custom(&next);
                    let update = match (&client, needs_client) {
                        (Some(client), true) => Some(
                            client
                                .prepare_update(&current, &next.program, &next.application)
                                .await?,
                        ),
                        _ => None,
                    };
                    let mut new_client = if client.is_none() && needs_client {
                        Some(
                            Box::pin(DevClient::start(
                                &current,
                                &next.program,
                                &next.application,
                                ClientBuildOptions::default(),
                                &origin,
                            ))
                            .await?,
                        )
                    } else {
                        None
                    };
                    if artifact::input_fingerprint(&current)?
                        != next.output.manifest.input_fingerprint
                    {
                        if let Some(client) = &mut new_client {
                            let _ = client.shutdown().await;
                        }
                        return Err(Error::InvalidConfig(
                            "开发候选准备期间源码发生变化，等待下一次保存后重新分析".into(),
                        ));
                    }
                    let publication = http
                        .publish_prepared(
                            next.output.application.clone(),
                            Some(next.output.client.clone()),
                            Some(next_js),
                            next.output.i18n.clone(),
                        )
                        .await;
                    if let Err(error) = publication {
                        if let Some(client) = &mut new_client {
                            let _ = client.shutdown().await;
                        }
                        return Err(error.into());
                    }
                    // 运行态和数据库已经提交；后续资源失败只能降级/报告，不能声称回滚。
                    committed = true;
                    project = current;
                    candidate = next;
                    observed = candidate.output.manifest.input_fingerprint.clone();
                    let mut reload = false;
                    if let Some(update) = update {
                        if let Some(client) = &mut client
                            && let Err(error) = client.commit_update(update, structural).await
                        {
                            tracing::error!(target:"kenvoy::lifecycle",%error,"模型已发布，但开发资源更新失败，将回退至已验证静态产物");
                            let _ = client.shutdown().await;
                            http.replace_development_proxy(None).await?;
                            reload = true;
                        }
                        if reload {
                            client = None;
                        }
                    } else if let Some(new_client) = new_client {
                        http.replace_development_proxy(Some(DevelopmentProxy {
                            endpoint: new_client.endpoint(),
                            secret: new_client.proxy_secret().to_owned(),
                        }))
                        .await?;
                        client = Some(new_client);
                        reload = true;
                    } else if !needs_client && let Some(mut old) = client.take() {
                        http.replace_development_proxy(None).await?;
                        old.shutdown().await?;
                        reload = true;
                    }
                    artifact::activate(&project, &candidate.output.manifest)?;
                    http.set_development_status(DevelopmentPhase::Ready, None, reload)
                        .await?;
                    rejected = false;
                    tracing::info!(target:"kenvoy::lifecycle","开发更新已发布");
                    Ok::<(), Error>(())
                };
                let result = tokio::select! {_=stopped.changed()=>break,result=changed=>result};
                if let Err(error) = result {
                    rejected = !committed;
                    tracing::warn!(target:"kenvoy::lifecycle",%error,"开发候选未完全发布，请查看诊断");
                    super::error::report_diagnostics(&error);
                    let _ = http
                        .set_development_status(
                            DevelopmentPhase::Error,
                            Some(if committed {"运行态已经更新，但开发资源或清单收尾失败；请查看终端并重新启动服务，不要重复提交。".into()} else {browser_error(&error)}),
                            false,
                        )
                        .await;
                }
            }
            http.replace_development_proxy(None).await?;
            if let Some(client) = &mut client {
                client.shutdown().await?;
            }
            Ok(())
        });
        Ok(Self { stop, task })
    }
    pub fn stop_signal(&self) -> watch::Sender<bool> {
        self.stop.clone()
    }
    pub async fn shutdown(self) -> Result<()> {
        const SHUTDOWN_LIMIT: Duration = Duration::from_secs(15);
        let _ = self.stop.send(true);
        let mut task = self.task;
        match tokio::time::timeout(SHUTDOWN_LIMIT, &mut task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::InvalidConfig("开发协调任务异常退出".into())),
            Err(_) => {
                task.abort();
                let _ = task.await;
                Err(Error::InvalidConfig(
                    "开发协调任务未能在期限内退出，已中止；请确认终端没有遗留的 Vite 进程".into(),
                ))
            }
        }
    }
}
fn custom(candidate: &Candidate) -> bool {
    candidate
        .application
        .module_references
        .iter()
        .any(|reference| reference.kind != ModuleReferenceKind::ServerFunction)
}
fn proxy(client: Option<&DevClient>) -> Option<DevelopmentProxy> {
    client.map(|client| DevelopmentProxy {
        endpoint: client.endpoint(),
        secret: client.proxy_secret().to_owned(),
    })
}
fn relevant(path: &Path, root: &Path, output: &Path, data: &Path) -> bool {
    if path.starts_with(output) || path.starts_with(data) {
        return false;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    if relative == Path::new("language.schema.json")
        || (relative.components().count() == 1
            && relative.to_string_lossy().starts_with(".language.schema."))
    {
        return false;
    }
    !relative.components().any(|part| {
        part.as_os_str().to_str().is_some_and(|name| {
            matches!(name, ".git" | ".kenvoy" | "node_modules" | "target")
                || name.starts_with(".env")
        })
    })
}
fn browser_error(error: &Error) -> String {
    match error {
        Error::Language(items) => items
            .first()
            .map(|item| {
                format!(
                    "源码检查失败（{}），请查看终端中的文件位置与修复建议。",
                    item.code
                )
            })
            .unwrap_or_else(|| "源码检查失败，请查看终端。".into()),
        Error::Dsl(items) => items
            .first()
            .map(|item| {
                format!(
                    "应用声明检查失败（{}），当前仍保留上一次有效页面。",
                    item.code
                )
            })
            .unwrap_or_else(|| "应用声明检查失败，请查看终端。".into()),
        Error::Database(_) | Error::Migration(_) => {
            "数据库变更未能发布，请运行 kenvoy migrate status 查看计划；已有数据保留。".into()
        }
        Error::InvalidConfig(_) => {
            "项目配置变更未能应用，请查看终端；更改数据库或监听地址后需要重新启动。".into()
        }
        _ => "开发更新未完成，请查看终端诊断；当前页面仍可用于核对已保存的数据。".into(),
    }
}
