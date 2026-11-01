mod application;
mod artifact;
mod auth;
mod build;
mod client_build;
mod database;
mod dsl;
mod error;
mod frontend;
mod http;
mod javascript;
mod lifecycle;
mod project;
mod validation;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kengine::{AnalysisSession, ProgramIR};

use application::{ApplicationIR, FieldKind};
use error::{Error, Result};
use project::Project;

#[derive(Parser)]
#[command(
    name = "kenvoy",
    version,
    about = "使用 TypeScript/JSX 声明应用的 Rust 原生框架"
)]
struct Cli {
    /// 项目位置；默认从当前目录向上查找 kenvoy.toml。
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    /// 输出结构化日志；不会记录密码、Cookie 或表单内容。
    #[arg(long, global = true)]
    json_logs: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 创建包含客户登记示例的项目。
    Create { path: PathBuf },
    /// 检查源码、模块、声明、权限与路由，不连接数据库。
    Check {
        /// 验证 JSON 记录所对应的资源。
        #[arg(long, requires = "input")]
        resource: Option<String>,
        /// 额外离线验证一条 JSON 记录，不写入数据库。
        #[arg(long, requires = "resource")]
        input: Option<PathBuf>,
        /// 按部分更新规则验证，缺失字段不会补默认值。
        #[arg(long, requires = "input")]
        patch: bool,
    },
    /// 查看应用模型或指定资源的注册规则。
    Inspect {
        resource: Option<String>,
        /// 输出通用源码 IR（包含源代码，仅供本地开发检查）。
        #[arg(long)]
        generic: bool,
    },
    /// 预览、应用数据库迁移或读取迁移历史。
    Migrate {
        #[command(subcommand)]
        action: Option<MigrationCommand>,
    },
    /// 初始化管理员或由本机项目所有者创建账号。
    Admin {
        #[command(subcommand)]
        action: AdminCommand,
    },
    /// 构建版本化应用描述、客户端资产和服务端模块，不连接数据库。
    Build {
        /// 明确允许安装用户 package.json 已声明的依赖，不增加或改变依赖声明。
        #[arg(long)]
        install_deps: bool,
    },
    /// 加载生产产物并启动；有源码且需要更新时自动构建。
    Run {
        /// 自动构建时明确允许安装用户已声明的 npm 依赖。
        #[arg(long)]
        install_deps: bool,
    },
    /// 启动开发服务。
    Dev {
        /// 明确允许安装用户已声明的 npm 依赖。
        #[arg(long)]
        install_deps: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum AccountRole {
    User,
    Admin,
}

#[derive(Subcommand)]
enum AdminCommand {
    /// 首次创建管理员；已有管理员时拒绝覆盖。
    Init {
        #[arg(long)]
        username: String,
        /// 从标准输入读取一行密码，适用于受控自动化；默认隐藏交互输入。
        #[arg(long)]
        password_stdin: bool,
    },
    /// 创建普通用户或管理员；要求已完成首次初始化。
    AddUser {
        #[arg(long)]
        username: String,
        #[arg(long, value_enum, default_value = "user")]
        role: AccountRole,
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Subcommand)]
enum MigrationCommand {
    /// 只读预览差异；数据库不存在时不创建文件。
    Status,
    /// 应用安全迁移；破坏性计划需要精确校验值和一致性备份。
    Apply {
        #[arg(long)]
        confirm: Option<String>,
    },
    /// 只读显示已执行迁移及校验记录。
    History,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let logging = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    let installed = if cli.json_logs {
        logging.json().try_init()
    } else {
        logging.try_init()
    };
    if let Err(error) = installed {
        eprintln!("日志初始化失败：{error}");
        std::process::exit(1);
    }
    if let Err(error) = execute(cli).await {
        tracing::error!(target:"kenvoy::cli",%error,"命令执行失败");
        error::report_diagnostics(&error);
        std::process::exit(1);
    }
}

async fn execute(cli: Cli) -> Result<()> {
    if let Command::Create { path } = &cli.command {
        project::create(path)?;
        println!(
            "已创建项目：{}\n下一步：进入项目目录，执行 kenvoy dev",
            path.display()
        );
        return Ok(());
    }
    let project = Project::discover(&cli.project)?;
    if matches!(
        cli.command,
        Command::Migrate {
            action: Some(MigrationCommand::History)
        }
    ) {
        println!(
            "{}",
            serde_json::to_string_pretty(&database::migration::history_path(&project.database)?)?
        );
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
        println!(
            "构建完成：{}\n版本：{}\n清单：{}",
            output.application.name,
            output.manifest.build_id,
            project.out_dir.join("manifest.json").display()
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
                println!(
                    "记录验证通过（未写入数据库）：\n{}",
                    serde_json::to_string_pretty(&validated)?
                );
            }
            println!(
                "检查通过：{}，{} 个资源，{} 个页面，{} 个动作\n数据库路径：{}\n构建输出：{}",
                application.name,
                application.resources.len(),
                application.pages.len(),
                application.actions.len(),
                project.database.display(),
                project.out_dir.display()
            );
        }
        Command::Inspect { generic: true, .. } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &program
                        .ok_or_else(|| Error::InvalidConfig("通用 IR 检查需要源代码".into()))?
                )?
            )
        }
        Command::Inspect {
            resource: Some(name),
            ..
        } => inspect_resource(&application, &name)?,
        Command::Inspect { .. } => println!("{}", serde_json::to_string_pretty(&application)?),
        Command::Migrate { action } => {
            let plan = database::migration::preview(&project.database, &application)?;
            match action.unwrap_or(MigrationCommand::Status) {
                MigrationCommand::Status => println!("{}", serde_json::to_string_pretty(&plan)?),
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
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({"plan":plan,"database":info,"completed":true})
                        )?
                    );
                }
                MigrationCommand::History => unreachable!("history 已独立处理"),
            }
        }
        Command::Admin { action } => {
            let (username, stdin, role, initialize) = match action {
                AdminCommand::Init {
                    username,
                    password_stdin,
                } => (username, password_stdin, application::Role::Admin, true),
                AdminCommand::AddUser {
                    username,
                    role,
                    password_stdin,
                } => (
                    username,
                    password_stdin,
                    match role {
                        AccountRole::User => application::Role::User,
                        AccountRole::Admin => application::Role::Admin,
                    },
                    false,
                ),
            };
            let password = read_password(stdin)?;
            let database = database::Database::open(project.database.clone(), application).await?;
            let operation = async {
                let auth = auth::Auth::new(database.clone()).await?;
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
            println!("已创建账号：{}，角色：{:?}", user.username, user.role);
        }
        Command::Dev { .. } => unreachable!("dev 已独立处理"),
        Command::Build { .. } => unreachable!("build 已独立处理"),
        Command::Run { .. } => unreachable!("run 已独立处理"),
        Command::Create { .. } => unreachable!("create 已独立处理"),
    }
    Ok(())
}

fn read_password(from_stdin: bool) -> Result<String> {
    use std::io::{BufRead, IsTerminal, Read};
    if from_stdin {
        let mut bytes = Vec::new();
        let mut input = std::io::stdin().lock().take(131);
        input
            .read_until(b'\n', &mut bytes)
            .map_err(|source| error::io("标准输入", source))?;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.len() > 128 {
            return Err(Error::InvalidConfig(
                "密码 UTF-8 长度不能超过 128 字节".into(),
            ));
        }
        return String::from_utf8(bytes)
            .map_err(|_| Error::InvalidConfig("密码必须是有效 UTF-8".into()));
    }
    if !std::io::stdin().is_terminal() {
        return Err(Error::InvalidConfig(
            "非交互输入需明确使用 --password-stdin；不能通过命令行参数传密码".into(),
        ));
    }
    let first = rpassword::prompt_password("请输入密码（12—128 UTF-8 字节）：")
        .map_err(|source| error::io("终端", source))?;
    let second = rpassword::prompt_password("请再次输入密码：")
        .map_err(|source| error::io("终端", source))?;
    if first != second {
        return Err(Error::InvalidConfig("两次密码输入不一致".into()));
    }
    Ok(first)
}

async fn run_server(
    project: &Project,
    output: artifact::Loaded,
    development: Option<(AnalysisSession, build::Candidate)>,
) -> Result<()> {
    let bundle: javascript::PreparedBundle = serde_json::from_slice(&output.server_modules)?;
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
        let auth = auth::Auth::new(database.clone()).await?;
        let runtime = javascript::JavascriptRuntime::start(
            std::sync::Arc::new(bundle),
            javascript::Limits::default(),
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
        println!("Kenvoy 应用：{origin}\n管理登录：{origin}/admin/login");
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

fn compile(project: &Project, session: &mut AnalysisSession) -> Result<(ApplicationIR, ProgramIR)> {
    let update = project.analyze(session)?;
    if update.program.has_errors() {
        return Err(Error::Language(update.program.diagnostics));
    }
    let result: dsl::AnalyzeResult = dsl::analyze(
        &update.program,
        &project.entry,
        &project.config.project.name,
    );
    if result.has_errors() {
        return Err(Error::Dsl(result.diagnostics));
    }
    let application = result.application.ok_or(Error::Dsl(result.diagnostics))?;
    Ok((application, update.program))
}

fn inspect_resource(application: &ApplicationIR, name: &str) -> Result<()> {
    let resource = application
        .resource(name)
        .ok_or_else(|| Error::InvalidConfig(format!("资源不存在：{name}")))?;
    println!("资源：{}\n\n字段：", resource.name);
    for field in &resource.fields {
        let storage = match field.kind {
            FieldKind::Number if !field.integer => "REAL",
            FieldKind::Number | FieldKind::Boolean => "INTEGER",
            _ => "TEXT",
        };
        println!(
            "  {}\t{}\t{}",
            field.name,
            storage,
            if field.required { "必填" } else { "可选" }
        );
    }
    println!(
        "\n数据库表：{name}\n\n接口注册计划：\n  GET/POST /api/{name}\n  GET/PATCH/DELETE /api/{name}/:id\n\n后台：/admin/{name}\n\n授权策略（本命令没有登录身份，不代表当前用户可调用）："
    );
    for permission in &application.permissions {
        if permission.target == application::PermissionTarget::Resource(name.to_owned()) {
            println!(
                "  {:?} {:?}，字段：{}",
                permission.role,
                permission.operation,
                permission.fields.join(", ")
            );
        }
    }
    Ok(())
}
