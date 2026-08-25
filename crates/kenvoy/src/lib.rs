//! kenvoy — CLI / dev 服务器 / 静态构建（基于 kengine）。
//!
//! 默认含 `db` + `s3` feature（生产集成）。`--no-default-features` 可裁剪为仅 CLI/接线。

use clap::{Parser, Subcommand};
use kengine::{self, EngineInfo};

#[cfg(feature = "db")]
pub mod auth;
#[cfg(feature = "db")]
pub mod build_cmd;
#[cfg(feature = "db")]
pub mod cli_auth;
pub mod config;
#[cfg(all(feature = "db", feature = "s3"))]
pub mod dev;
pub mod init;
#[cfg(feature = "mail")]
pub mod mail;

#[cfg(feature = "db")]
pub mod db;
#[cfg(feature = "s3")]
pub mod s3;

/// kenvoy — CLI toolkit built on kengine.
#[derive(Parser, Debug)]
#[command(name = "kenvoy", version, about, long_about = None)]
pub struct Cli {
    /// 项目根目录（含 `kenvoy.toml`，auth 管理命令使用）
    #[arg(long, global = true, default_value = ".")]
    pub dir: std::path::PathBuf,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 打印问候语
    Hello {
        /// 问候对象
        #[arg(default_value = "world")]
        name: String,
    },
    /// 显示引擎信息
    Info,
    /// 启动开发服务器：扫描 apps/ → 解析渲染 → 浏览器实时预览
    #[cfg(all(feature = "db", feature = "s3"))]
    Dev {
        /// 项目根目录（包含 apps/ 与 public/）
        #[arg(long, default_value = ".")]
        dir: std::path::PathBuf,
        /// 监听端口（缺省用 kenvoy.toml 的 [server].port，再缺省 5173）
        #[arg(long)]
        port: Option<u16>,
    },
    /// 初始化新项目骨架
    Init {
        /// 项目目录名（缺省为当前目录）
        name: Option<String>,
        /// 项目显示名称（缺省用目录名）
        #[arg(long)]
        title: Option<String>,
    },
    /// 构建静态产物到 dist/
    #[cfg(feature = "db")]
    Build {
        /// 项目根目录
        #[arg(long, default_value = ".")]
        dir: std::path::PathBuf,
        /// 覆盖输出目录（缺省用 kenvoy.toml 的 [build].output，再缺省 dist）
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// 部署到 KenvoySpace（暂未开放）
    Deploy {
        /// 项目根目录
        #[arg(long, default_value = ".")]
        dir: std::path::PathBuf,
    },
    /// 用户管理
    #[cfg(feature = "db")]
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// 角色管理（账号等级）
    #[cfg(feature = "db")]
    Role {
        #[command(subcommand)]
        cmd: RoleCmd,
    },
    /// 通讯录管理
    #[cfg(feature = "db")]
    Contact {
        #[command(subcommand)]
        cmd: ContactCmd,
    },
    /// 部门管理
    #[cfg(feature = "db")]
    Department {
        #[command(subcommand)]
        cmd: DepartmentCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum UserCmd {
    /// 创建用户
    Add {
        /// 登录名
        #[arg(long)]
        username: String,
        /// 密码
        #[arg(long)]
        password: String,
        /// 显示名
        #[arg(long)]
        display_name: String,
        /// 分配角色（按名称）
        #[arg(long)]
        role: Option<String>,
    },
    /// 列出用户
    List,
}

#[derive(Subcommand, Debug)]
pub enum RoleCmd {
    /// 创建角色
    Add {
        /// 角色名
        #[arg(long)]
        name: String,
        /// 账号等级
        #[arg(long)]
        level: i32,
        /// 描述
        #[arg(long)]
        description: Option<String>,
    },
    /// 列出角色
    List,
}

#[derive(Subcommand, Debug)]
pub enum ContactCmd {
    /// 新增通讯录人员
    Add {
        /// 姓名
        #[arg(long)]
        name: String,
        /// 部门（按名称）
        #[arg(long)]
        department: Option<String>,
        /// 关联账号（按用户名，可空=无账号人员）
        #[arg(long)]
        user: Option<String>,
        /// 邮箱
        #[arg(long)]
        email: Option<String>,
        /// 手机
        #[arg(long)]
        phone: Option<String>,
        /// 职务
        #[arg(long)]
        title: Option<String>,
    },
    /// 列出通讯录
    List {
        /// 姓名关键字过滤
        #[arg(long)]
        keyword: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DepartmentCmd {
    /// 创建部门
    Add {
        /// 部门名
        #[arg(long)]
        name: String,
        /// 上级部门（按名称）
        #[arg(long)]
        parent: Option<String>,
    },
    /// 列出部门
    List,
}

/// kenvoy CLI 入口：解析子命令并执行。
///
/// # Panics
/// 当 `Cli::parse` 解析参数失败时（已被 clap 内部处理为帮助输出）。
///
/// # Errors
/// 任何子命令执行失败时返回错误。
#[tokio::main]
#[allow(clippy::too_many_lines)] // CLI 入口：子命令分支较多
pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Hello { name }) => {
            println!("{}", kengine::hello(&name));
        }
        Some(Commands::Info) => {
            let info = EngineInfo::new();
            println!("{info}");
        }
        #[cfg(all(feature = "db", feature = "s3"))]
        Some(Commands::Dev { dir, port }) => {
            let cfg = config::read_config(&dir);
            let name = config::resolve_name(&dir, &cfg, None);
            let host = cfg.server.host.as_deref().unwrap_or("127.0.0.1");
            let port = port.or(cfg.server.port).unwrap_or(5173);
            let db = match &cfg.database.url {
                Some(url) => match db::connect(url).await {
                    Ok(pool) => match db::migrate(&pool).await {
                        Ok(()) => {
                            println!("已连接 PostgreSQL 并完成迁移");
                            Some(pool)
                        }
                        Err(err) => {
                            eprintln!("数据库迁移失败（使用内存模式）：{err}");
                            None
                        }
                    },
                    Err(err) => {
                        eprintln!("连接数据库失败（使用内存模式）：{err}");
                        None
                    }
                },
                None => None,
            };
            dev::run(dev::DevOptions {
                dir: &dir,
                host,
                port,
                project_name: &name,
                description: cfg.project.description.clone(),
                locale: cfg.project.locale.clone(),
                storage: &cfg.storage,
                auth: &cfg.auth,
                mail: &cfg.mail,
                db,
            })
            .await?;
        }
        Some(Commands::Init { name, title }) => {
            let target =
                name.map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from);
            init::run(&target, title.as_deref())?;
        }
        #[cfg(feature = "db")]
        Some(Commands::Build { dir, out }) => {
            let cfg = config::read_config(&dir);
            build_cmd::run(&dir, out.as_deref(), &cfg).await?;
        }
        Some(Commands::Deploy { dir: _ }) => {
            println!("KenvoySpace 为闭源商业产品，部署命令暂未开放，请关注后续版本。");
        }
        #[cfg(feature = "db")]
        Some(Commands::User { cmd }) => {
            let dir = cli.dir.clone();
            match cmd {
                UserCmd::Add {
                    username,
                    password,
                    display_name,
                    role,
                } => {
                    cli_auth::user_add(&dir, &username, &password, &display_name, role.as_deref())
                        .await?;
                }
                UserCmd::List => cli_auth::user_list(&dir).await?,
            }
        }
        #[cfg(feature = "db")]
        Some(Commands::Role { cmd }) => {
            let dir = cli.dir.clone();
            match cmd {
                RoleCmd::Add {
                    name,
                    level,
                    description,
                } => cli_auth::role_add(&dir, &name, level, description.as_deref()).await?,
                RoleCmd::List => cli_auth::role_list(&dir).await?,
            }
        }
        #[cfg(feature = "db")]
        Some(Commands::Contact { cmd }) => {
            let dir = cli.dir.clone();
            match cmd {
                ContactCmd::Add {
                    name,
                    department,
                    user,
                    email,
                    phone,
                    title,
                } => {
                    cli_auth::contact_add(
                        &dir,
                        &name,
                        department.as_deref(),
                        user.as_deref(),
                        email.as_deref(),
                        phone.as_deref(),
                        title.as_deref(),
                    )
                    .await?;
                }
                ContactCmd::List { keyword } => {
                    cli_auth::contact_list(&dir, keyword.as_deref()).await?;
                }
            }
        }
        #[cfg(feature = "db")]
        Some(Commands::Department { cmd }) => {
            let dir = cli.dir.clone();
            match cmd {
                DepartmentCmd::Add { name, parent } => {
                    cli_auth::dept_add(&dir, &name, parent.as_deref()).await?;
                }
                DepartmentCmd::List => {
                    cli_auth::dept_list(&dir).await?;
                }
            }
        }
        None => {
            // 默认行为：提示 help
            println!("kenvoy {} — use --help for usage", kengine::VERSION);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_hello() {
        let cli = Cli::try_parse_from(["kenvoy", "hello", "alice"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Hello { name }) if name == "alice"));
    }

    #[test]
    fn cli_parses_info() {
        let cli = Cli::try_parse_from(["kenvoy", "info"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Info)));
    }

    #[test]
    fn cli_parses_dev() {
        let cli = Cli::try_parse_from(["kenvoy", "dev", "--port", "8080"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Dev {
                port: Some(8080),
                ..
            })
        ));
    }

    #[test]
    fn cli_parses_init() {
        let cli = Cli::try_parse_from(["kenvoy", "init", "my-app", "--title", "我的应用"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Init { name: Some(n), title: Some(t), .. })
                if n == "my-app" && t == "我的应用"
        ));
    }

    #[test]
    fn cli_parses_build() {
        let cli = Cli::try_parse_from(["kenvoy", "build"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Build { .. })));
    }

    #[test]
    fn cli_parses_deploy() {
        let cli = Cli::try_parse_from(["kenvoy", "deploy"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Deploy { .. })));
    }
}
