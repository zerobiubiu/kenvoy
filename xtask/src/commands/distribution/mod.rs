//! 将明确指定的 release 产物校验后归档，不构建或发布产品。

mod archive;
mod binary;
mod options;
mod target;
mod temporary;
mod version;

use crate::Result;
use std::{ffi::OsString, fs, path::Path};

pub(super) fn execute(root: &Path, arguments: &[OsString]) -> Result<()> {
    let mut options = options::Options::parse(root, arguments)?;
    options.binary = fs::canonicalize(&options.binary)?;
    binary::verify(&options.binary, options.target)?;
    version::verify(&options)?;
    archive::create(&options)
}
