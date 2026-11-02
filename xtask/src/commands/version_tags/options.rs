//! 版本标签命令的只读默认值与显式仓库选择。

use crate::Result;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

pub(super) struct Options {
    pub(super) repository: PathBuf,
    pub(super) apply: bool,
    pub(super) custom_repository: bool,
}

impl Options {
    pub(super) fn parse(root: &Path, arguments: &[OsString]) -> Result<Self> {
        let mut options = Self {
            repository: root.to_path_buf(),
            apply: false,
            custom_repository: false,
        };
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--apply") if !options.apply => options.apply = true,
                Some("--repo") if !options.custom_repository => {
                    let path = arguments.next().ok_or("--repo 缺少仓库路径")?;
                    if path.is_empty() || path.to_string_lossy().starts_with("--") {
                        return Err("--repo 必须指定非空仓库路径".into());
                    }
                    options.repository = PathBuf::from(path);
                    options.custom_repository = true;
                }
                Some("--apply" | "--repo") => {
                    return Err(format!("参数重复：{}", argument.to_string_lossy()).into());
                }
                _ => {
                    return Err(format!(
                        "未知版本标签参数：{}；请运行 cargo xtask version-tags --help",
                        argument.to_string_lossy()
                    )
                    .into());
                }
            }
        }
        Ok(options)
    }
}
