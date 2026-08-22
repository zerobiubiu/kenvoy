use clap::{Parser, Subcommand};
use kengine::{self, EngineInfo};

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
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Hello { name }) => {
            println!("{}", kengine::hello(&name));
        }
        Some(Commands::Info) => {
            let info = EngineInfo::new();
            println!("{info}");
        }
        None => {
            // 默认行为：提示 help
            println!("kenvoy {} — use --help for usage", kengine::VERSION);
        }
    }
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
}
