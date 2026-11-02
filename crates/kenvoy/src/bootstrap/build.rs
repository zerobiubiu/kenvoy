//! 分析、服务端模块与客户端资产在同一候选快照完成后才提交产物清单。
use kengine::{AnalysisSession, ProgramIR};

use super::error::{Error, Result};
use crate::infrastructure::{
    artifacts as artifact, client_build,
    frontend::{CARGO_LOCK_SHA256, KENGINE_VERSION},
    javascript,
    project::Project,
};

pub struct Candidate {
    pub output: artifact::Loaded,
    pub application: crate::domain::model::ApplicationIR,
    pub program: ProgramIR,
}

pub async fn prepare(
    project: &Project,
    session: &mut AnalysisSession,
    install_deps: bool,
) -> Result<artifact::Loaded> {
    let candidate = candidate(project, session, install_deps).await?;
    artifact::activate(project, &candidate.output.manifest)?;
    Ok(candidate.output)
}

pub async fn candidate(
    project: &Project,
    session: &mut AnalysisSession,
    install_deps: bool,
) -> Result<Candidate> {
    // 显式安装用户依赖可能首次生成 lock；最多重新分析一次，不发布前后不一致的源码。
    for attempt in 0..2 {
        let stage = artifact::Staging::new(project)?;
        let before = artifact::input_fingerprint(project)?;
        let (application, program) = super::compile(project, session)?;
        let server = javascript::prepare(&program, &application, project.config.runtime.trust)?;
        let client = client_build::build(
            project,
            &program,
            &application,
            &stage.client(),
            client_build::ClientBuildOptions {
                install_deps: install_deps && attempt == 0,
            },
        )
        .await?;
        let after = artifact::input_fingerprint(project)?;
        let sources_unchanged = program.modules.iter().all(|module| {
            std::fs::read_to_string(&module.path).is_ok_and(|source| source == module.source)
        });
        if before != after || !sources_unchanged {
            if attempt == 0 {
                tracing::info!(target:"kenvoy::compiler","构建期间输入发生变化，重新分析一次后再发布");
                continue;
            }
            return Err(Error::InvalidConfig(
                "构建期间源码持续变化，候选产物未发布；请保存完成后重试".into(),
            ));
        }
        let mut dependencies = client.dependencies;
        dependencies.insert("rust:kengine".into(), KENGINE_VERSION.into());
        dependencies.insert("rust:swc_ecma_parser".into(), "47.0.0".into());
        dependencies.insert("rust:rquickjs".into(), "0.14.0".into());
        dependencies.insert("rust:rusqlite".into(), "0.41.1".into());
        dependencies.insert("sqlite".into(), rusqlite::version().into());
        dependencies.insert(
            "framework:cargo_lock_sha256".into(),
            CARGO_LOCK_SHA256.into(),
        );
        let loaded = stage.seal(
            &application,
            &serde_json::to_vec(&server)?,
            after,
            dependencies,
        )?;
        tracing::info!(target:"kenvoy::compiler",build_id=%loaded.manifest.build_id,files=loaded.manifest.files.len(),"应用构建完成");
        return Ok(Candidate {
            output: loaded,
            application,
            program,
        });
    }
    Err(Error::InvalidConfig("构建未能获得一致快照".into()))
}
