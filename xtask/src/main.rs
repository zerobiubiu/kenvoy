//! 仓库工程任务；不安装依赖，不参与应用运行。

mod checks;
mod commands;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

fn main() {
    if let Err(error) = commands::execute() {
        eprintln!("工程任务失败：{error}");
        std::process::exit(1);
    }
}
