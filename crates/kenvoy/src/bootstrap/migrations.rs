//! 命令只编排正式交付链；生成与固化不连接业务数据库。
use std::path::Path;

use kengine::AnalysisSession;

use super::{
    compile,
    error::{Error, Result},
};
use crate::{
    domain::model::ApplicationIR,
    infrastructure::{
        artifacts,
        persistence::sqlite::{
            self,
            lifecycle::DatabaseLease,
            migration::catalog::{self, Catalog},
            snapshot,
        },
        project::Project,
    },
    interfaces::cli::{self, MigrationCommand},
};

pub(super) fn catalog_for(project: &Project, application: &ApplicationIR) -> Result<Catalog> {
    let catalog = catalog::read(&project.root)?;
    catalog.validate()?;
    catalog.validate_target(application)?;
    Ok(catalog)
}

pub(super) fn initialize(project: &Project, session: &mut AnalysisSession) -> Result<()> {
    project.check_compatibility()?;
    let (application, _) = compile(project, session)?;
    catalog::generate(&project.root, &application, "初始化应用结构", None)?;
    catalog::seal(&project.root, &application, None)?;
    Ok(())
}

pub(super) fn application_and_catalog(
    project: &Project,
    session: &mut AnalysisSession,
) -> Result<(ApplicationIR, Catalog)> {
    if project.entry.is_file() {
        let (application, _) = compile(project, session)?;
        let catalog = catalog_for(project, &application)?;
        Ok((application, catalog))
    } else {
        let output = artifacts::load(project)?;
        Ok((output.application, output.migrations))
    }
}

fn rules_path(project: &Project, path: Option<&Path>) -> Option<std::path::PathBuf> {
    path.map(|path| {
        if path.is_absolute() {
            path.into()
        } else {
            project.root.join(path)
        }
    })
}

pub(super) async fn execute(
    project: &Project,
    action: MigrationCommand,
    session: &mut AnalysisSession,
) -> Result<()> {
    match action {
        MigrationCommand::Generate { description, rules } => {
            let (application, _) = compile(project, session)?;
            let rules = rules_path(project, rules.as_deref());
            cli::print_json(&catalog::generate(
                &project.root,
                &application,
                &description,
                rules.as_deref(),
            )?)?;
        }
        MigrationCommand::Seal { confirm } => {
            let (application, _) = compile(project, session)?;
            cli::print_json(&catalog::seal(
                &project.root,
                &application,
                confirm.as_deref(),
            )?)?;
        }
        MigrationCommand::Status => {
            let (_, catalog) = application_and_catalog(project, session)?;
            cli::print_json(&catalog::status(&project.database, &catalog)?)?;
        }
        MigrationCommand::Apply { confirm } => {
            let (application, catalog) = application_and_catalog(project, session)?;
            snapshot::reject(&project.database)?;
            let database = sqlite::Database::open_catalog(
                project.database.clone(),
                application,
                catalog,
                confirm,
            )
            .await?;
            let info = database.info().await;
            let shutdown = database.shutdown().await;
            let info = info?;
            shutdown?;
            cli::print_json(&serde_json::json!({"database":info,"completed":true}))?;
        }
        MigrationCommand::Baseline { at, rules, confirm } => {
            let (_, catalog) = application_and_catalog(project, session)?;
            snapshot::reject(&project.database)?;
            let rules = rules_path(project, rules.as_deref());
            // 预览不创建目录与锁文件；确认写入时与服务使用相同生命周期锁。
            let _lease = match confirm {
                Some(_) => Some(DatabaseLease::acquire(&project.database)?),
                None => None,
            };
            cli::print_json(&catalog::baseline(
                &project.database,
                &catalog,
                &at,
                rules.as_deref(),
                confirm.as_deref(),
            )?)?;
        }
        MigrationCommand::History => {
            return Err(Error::InvalidConfig("迁移历史应由独立只读入口处理".into()));
        }
    }
    Ok(())
}
