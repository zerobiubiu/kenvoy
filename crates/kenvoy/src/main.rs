use clap::{Parser, Subcommand};
use kengine::{self, EngineInfo};

mod build_cmd;
mod config;
mod dev;
mod init;

/// kenvoy — CLI toolkit built on kengine.
#[derive(Parser, Debug)]
#[command(name = "kenvoy", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 打印问候语
    Hello {
        /// 问候对象
        #[arg(default_value = "world")]
        name: String,
    },
    /// 显示引擎信息
    Info,
    /// 启动开发服务器：扫描 apps/ → 解析渲染 → 浏览器实时预览
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Hello { name }) => {
            println!("{}", kengine::hello(&name));
        }
        Some(Commands::Info) => {
            let info = EngineInfo::new();
            println!("{info}");
        }
        Some(Commands::Dev { dir, port }) => {
            let cfg = config::read_config(&dir);
            let name = config::resolve_name(&dir, &cfg, None);
            let host = cfg.server.host.as_deref().unwrap_or("127.0.0.1");
            let port = port.or(cfg.server.port).unwrap_or(5173);
            let db = match &cfg.database.url {
                Some(url) => match kengine_db::connect(url).await {
                    Ok(pool) => match kengine_db::migrate(&pool).await {
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
            dev::run(&dir, host, port, &name, cfg.project.description.clone(), db).await?;
        }
        Some(Commands::Init { name, title }) => {
            let target =
                name.map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from);
            init::run(&target, title.as_deref())?;
        }
        Some(Commands::Build { dir, out }) => {
            let cfg = config::read_config(&dir);
            build_cmd::run(&dir, out.as_deref(), &cfg)?;
        }
        Some(Commands::Deploy { dir: _ }) => {
            println!("KenvoySpace 为闭源商业产品，部署命令暂未开放，请关注后续版本。");
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
