use kengine::AnalysisSession;

use super::{
    build, compile,
    error::{self, Error, Result},
    i18n, migrations,
    server::run_server,
    signals::Shutdown,
};
use crate::{
    application::identity::Auth,
    domain::{model, validation},
    infrastructure::{
        artifacts as artifact,
        identity::SystemCredentials,
        javascript,
        persistence::sqlite as database,
        project::{self, Project},
    },
    interfaces::cli::{self, AccountRole, AdminCommand, Cli, Command, MigrationCommand},
};

pub(crate) async fn execute(cli: Cli) -> Result<()> {
    if let Command::Create { path } = &cli.command {
        project::create(path)?;
        let project = Project::load(path)?;
        migrations::initialize(&project, &mut AnalysisSession::new())?;
        cli::created(path);
        return Ok(());
    }
    let mut project = Project::discover(&cli.project)?;
    if let Command::Types = cli.command {
        cli::print_json(&project::editor::prepare(&project)?)?;
        return Ok(());
    }
    if matches!(
        cli.command,
        Command::Migrate {
            action: Some(MigrationCommand::History)
        }
    ) {
        // 历史诊断不执行项目代码，也不要求旧配置已完成格式接入。
        cli::print_json(&database::migration::catalog::history(&project.database)?)?;
        return Ok(());
    }
    project.check_compatibility()?;
    let mut session = AnalysisSession::new();
    if let Command::I18n = cli.command {
        return i18n::execute(&project);
    }
    if let Command::Run {
        install_deps,
        confirm,
    } = cli.command
    {
        project::i18n::prepare(&project, false)?;
        let shutdown = Shutdown::install();
        let output = if project.entry.is_file() {
            let fingerprint = artifact::input_fingerprint(&project)?;
            match artifact::load(&project) {
                Ok(output) if output.manifest.input_fingerprint == fingerprint => output,
                _ => tokio::select! {
                    result = build::prepare(&project, &mut session, install_deps) => result?,
                    _ = shutdown.wait() => {
                        tracing::info!(target:"kenvoy::lifecycle","启动阶段收到终止信号，已取消构建");
                        return Ok(());
                    }
                },
            }
        } else {
            artifact::load(&project)?
        };
        database::snapshot::reject(&project.database)?;
        return Box::pin(run_server(&project, output, None, shutdown, confirm)).await;
    }
    if let Command::Build { install_deps } = cli.command {
        let output = build::prepare(&project, &mut session, install_deps).await?;
        cli::build_completed(
            &output.application.name,
            &output.manifest.build_id,
            &project.out_dir.join("manifest.json"),
        );
        return Ok(());
    }
    if let Command::Dev {
        install_deps,
        snapshot,
        confirm,
    } = cli.command
    {
        project::i18n::prepare(&project, false)?;
        if let Some(path) = snapshot {
            project.use_snapshot(&path)?;
        }
        let shutdown = Shutdown::install();
        let candidate = tokio::select! {
            result = build::candidate(&project, &mut session, install_deps) => result?,
            _ = shutdown.wait() => {
                tracing::info!(target:"kenvoy::lifecycle","启动阶段收到终止信号，已取消构建");
                return Ok(());
            }
        };
        return Box::pin(run_server(
            &project,
            candidate.output.clone(),
            Some((session, candidate)),
            shutdown,
            confirm,
        ))
        .await;
    }
    if let Command::Migrate { action } = cli.command {
        return migrations::execute(
            &project,
            action.unwrap_or(MigrationCommand::Status),
            &mut session,
        )
        .await;
    }
    if let Command::Admin { action } = cli.command {
        let (application, catalog) = migrations::application_and_catalog(&project, &mut session)?;
        return admin(&project, application, catalog, action).await;
    }
    if !project.entry.is_file() && matches!(cli.command, Command::Inspect { generic: false, .. }) {
        let application = artifact::load(&project)?.application;
        return match cli.command {
            Command::Inspect {
                resource: Some(name),
                ..
            } => cli::inspect_resource(&application, &name).map_err(Error::from),
            _ => cli::print_json(&application).map_err(Error::from),
        };
    }
    let (application, program) = compile(&project, &mut session)?;
    match cli.command {
        Command::Check {
            resource,
            input,
            patch,
        } => {
            let i18n = project::i18n::load(&project)?;
            javascript::validate_i18n(&program, &i18n).map_err(Error::Dsl)?;
            javascript::prepare(&program, &application, project.config.runtime.trust)?;
            if let (Some(name), Some(path)) = (resource, input) {
                let resource = application
                    .resource(&name)
                    .ok_or_else(|| Error::InvalidConfig(format!("资源不存在：{name}")))?;
                let bytes = std::fs::read(&path).map_err(|error| error::io(&path, error))?;
                let data = validation::parse_record_json(&bytes)?;
                let validated = validation::validate_record(
                    resource,
                    &data,
                    if patch {
                        validation::WriteMode::Patch
                    } else {
                        validation::WriteMode::Create
                    },
                )?;
                cli::record_validated(&validated)?;
            }
            cli::check_succeeded(&application, &project.database, &project.out_dir);
        }
        Command::Inspect { generic: true, .. } => cli::print_json(&program)?,
        Command::Inspect {
            resource: Some(name),
            ..
        } => cli::inspect_resource(&application, &name)?,
        Command::Inspect { .. } => cli::print_json(&application)?,
        _ => unreachable!("其他命令已独立处理"),
    }
    Ok(())
}

async fn admin(
    project: &Project,
    application: model::ApplicationIR,
    catalog: database::migration::catalog::Catalog,
    action: AdminCommand,
) -> Result<()> {
    let (username, stdin, role, initialize) = match action {
        AdminCommand::Init {
            username,
            password_stdin,
        } => (username, password_stdin, model::Role::Admin, true),
        AdminCommand::AddUser {
            username,
            role,
            password_stdin,
        } => (
            username,
            password_stdin,
            match role {
                AccountRole::User => model::Role::User,
                AccountRole::Admin => model::Role::Admin,
            },
            false,
        ),
    };
    let password = cli::read_password(stdin)?;
    let database =
        database::Database::open_catalog(project.database.clone(), application, catalog, None)
            .await?;
    let operation = async {
        let auth = Auth::new(database.clone(), SystemCredentials).await?;
        if initialize {
            auth.init_admin(&username, &password).await
        } else {
            auth.add_user(&username, &password, role).await
        }
    }
    .await;
    let shutdown = database.shutdown().await;
    let user = operation?;
    shutdown?;
    cli::account_created(&user);
    Ok(())
}
