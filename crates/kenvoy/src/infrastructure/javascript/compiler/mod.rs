//! 服务端模块的 TypeScript 转换与可持久化白名单；不在编译阶段执行项目代码。

mod parser;
mod prepare;
mod validation;

use crate::application::{
    compilation::client_slice::ClientCodeTransformer, execution::JsDiagnostic,
};

pub use prepare::prepare;
pub use validation::validate;

#[derive(Clone, Copy, Default)]
pub struct SwcClientCodeTransformer;

impl ClientCodeTransformer for SwcClientCodeTransformer {
    fn transpile(&self, source: &str, name: &str, jsx: bool) -> Result<String, JsDiagnostic> {
        parser::transpile(source, name, jsx)
    }
}

const MAX_MODULES: usize = 256;
const MAX_FUNCTIONS: usize = 1024;
const MAX_MODULE_BYTES: usize = 512 * 1024;
const MAX_BUNDLE_BYTES: usize = 4 * 1024 * 1024;

fn failure(message: impl Into<String>) -> JsDiagnostic {
    JsDiagnostic::new(
        "KV6001",
        message,
        None,
        "使用可信项目中的本地静态 ES Module，检查登记的函数与依赖",
    )
}

fn valid_specifier(value: &str) -> bool {
    (value.starts_with("./") || value.starts_with("../"))
        && value.len() <= 512
        && !value.contains(['\0', '\\', ':', '?', '#'])
}

#[cfg(test)]
mod tests;
