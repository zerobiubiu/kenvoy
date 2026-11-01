//! 文件、数据库、编译器与运行时的实际技术适配；依赖应用端口和领域规则。

pub(crate) mod artifacts;
pub(crate) mod client_build;
pub(crate) mod frontend;
pub(crate) mod identity;
pub(crate) mod javascript;
pub(crate) mod persistence;
pub(crate) mod project;
