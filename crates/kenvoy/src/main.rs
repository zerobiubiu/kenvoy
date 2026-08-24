use clap::{Parser, Subcommand};
use kengine::{self, EngineInfo};

mod dev;

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
        /// 监听端口
        #[arg(long, default_value_t = 5173)]
        port: u16,
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
            dev::run(&dir, port).await?;
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
            Some(Commands::Dev { port: 8080, .. })
        ));
    }
}
