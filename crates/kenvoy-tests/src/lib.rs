//! 仅供验收的隔离项目与真实子进程夹具，不被产品 Crate 依赖。

mod fixture;
mod process;
#[cfg(windows)]
mod windows_job;

pub use fixture::{Fixture, Server, require_success};
pub use process::ProcessGuard;

pub type TestResult<T = (), E = Box<dyn std::error::Error + Send + Sync>> = Result<T, E>;

#[cfg(test)]
mod tests;
