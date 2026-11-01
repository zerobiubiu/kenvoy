use kengine::AnalysisSession;

use super::{
    build, compile,
    error::{self, Error, Result},
    server::run_server,
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
        cli::created(path);
        return Ok(());
    }
    let project = Project::discover(&cli.project)?;
    if matches!(
        cli.command,
        Command::Migrate {
            action: Some(MigrationCommand::History)
        }
    ) {
        cli::print_json(&database::migration::history_path(&project.database)?)?;
        return Ok(());
    }
    let mut session = AnalysisSession::new();
    if let Command::Run { install_deps } = cli.command {
        let output = if project.entry.is_file() {
            let fingerprint = artifact::input_fingerprint(&project)?;
            match artifact::load(&project) {
                Ok(output) if output.manifest.input_fingerprint == fingerprint => output,
                _ => build::prepare(&project, &mut session, install_deps).await?,
            }
        } else {
            artifact::load(&project)?
        };
        return Box::pin(run_server(&project, output, None)).await;
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
    if let Command::Dev { install_deps } = cli.command {
        let candidate = build::candidate(&project, &mut session, install_deps).await?;
        return Box::pin(run_server(
            &project,
            candidate.output.clone(),
            Some((session, candidate)),
        ))
        .await;
    }
    let (application, program) = if !project.entry.is_file()
        && !matches!(
            cli.command,
            Command::Check { .. } | Command::Inspect { generic: true, .. }
        ) {
        (artifact::load(&project)?.application, None)
    } else {
        let (application, program) = compile(&project, &mut session)?;
        (application, Some(program))
    };
    match cli.command {
        Command::Check {
            resource,
            input,
            patch,
        } => {
            let program = program
                .as_ref()
                .ok_or_else(|| Error::InvalidConfig("check 需要源代码".into()))?;
            javascript::prepare(program, &application, project.config.runtime.trust)?;
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
        Command::Inspect { generic: true, .. } => {
            cli::print_json(
                &program.ok_or_else(|| Error::InvalidConfig("通用 IR 检查需要源代码".into()))?,
            )?;
        }
        Command::Inspect {
            resource: Some(name),
            ..
        } => cli::inspect_resource(&application, &name)?,
        Command::Inspect { .. } => cli::print_json(&application)?,
        Command::Migrate { action } => {
            let plan = database::migration::preview(&project.database, &application)?;
            match action.unwrap_or(MigrationCommand::Status) {
                MigrationCommand::Status => cli::print_json(&plan)?,
                MigrationCommand::Apply { confirm } => {
                    let database = if confirm.is_some() {
                        database::Database::open_confirmed(
                            project.database.clone(),
                            application,
                            confirm,
                        )
                        .await?
                    } else {
                        database::Database::open(project.database.clone(), application).await?
                    };
                    let info: database::Result<database::DatabaseInfo> = database.info().await;
                    let shutdown = database.shutdown().await;
                    let info = info?;
                    shutdown?;
                    cli::print_json(
                        &serde_json::json!({"plan":plan,"database":info,"completed":true}),
                    )?;
                }
                MigrationCommand::History => unreachable!("history 已独立处理"),
            }
        }
        Command::Admin { action } => {
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
            let database = database::Database::open(project.database.clone(), application).await?;
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
        }
        Command::Dev { .. } => unreachable!("dev 已独立处理"),
        Command::Build { .. } => unreachable!("build 已独立处理"),
        Command::Run { .. } => unreachable!("run 已独立处理"),
        Command::Create { .. } => unreachable!("create 已独立处理"),
    }
    Ok(())
}
