//! CLI 参数、受保护密码输入与终端渲染。
mod args;
mod error;
mod output;
mod password;

pub(crate) use args::{AccountRole, AdminCommand, Cli, Command, MigrationCommand};
pub(crate) use error::CliError;
pub(crate) use output::{
    account_created, build_completed, check_succeeded, created, inspect_resource, print_json,
    record_validated, server_ready,
};
pub(crate) use password::read_password;
