use std::path::PathBuf;

use thiserror::Error;

/// 无法读取输入等宿主错误；语法与依赖错误通过结果中的诊断返回。
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("读取或定位源文件 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("模块 {specifier} 未能从 {importer} 解析到源码文件")]
    NotFound {
        importer: PathBuf,
        specifier: String,
    },
    #[error("模块路径 {path} 超出分析根目录 {root}")]
    OutsideRoot { path: PathBuf, root: PathBuf },
    #[error("不支持模块说明符 {specifier}；相对路径或显式 external 才可解析")]
    UnsupportedSpecifier { specifier: String },
    #[error("定位模块路径 {path} 失败：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
