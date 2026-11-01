mod application;
mod bootstrap;
mod domain;
mod infrastructure;
mod interfaces;

use clap::Parser;
use interfaces::cli::Cli;

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
    if let Err(error) = bootstrap::execute(cli).await {
        tracing::error!(target:"kenvoy::cli",%error,"命令执行失败");
        bootstrap::report_diagnostics(&error);
        std::process::exit(1);
    }
}
