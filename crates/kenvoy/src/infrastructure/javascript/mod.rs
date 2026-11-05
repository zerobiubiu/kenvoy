//! 可信项目的受限 JavaScript 适配；不提供 Node、文件、网络或数据库宿主能力。

mod compiler;
mod runtime;

pub use compiler::{SwcClientCodeTransformer, prepare, validate, validate_i18n};
pub use runtime::JavascriptRuntime;
