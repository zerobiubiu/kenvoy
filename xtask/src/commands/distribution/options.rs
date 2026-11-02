//! 分发命令只接收既有产物与明确的归档位置。

use super::target::Target;
use crate::Result;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

pub(super) struct Options {
    pub(super) binary: PathBuf,
    pub(super) target: Target,
    pub(super) version: String,
    pub(super) output: PathBuf,
    pub(super) archive_name: String,
    pub(super) readme: PathBuf,
    pub(super) license: PathBuf,
}

impl Options {
    pub(super) fn parse(root: &Path, arguments: &[OsString]) -> Result<Self> {
        let mut binary = None;
        let mut target = None;
        let mut version = None;
        let mut output = None;
        let mut archive_name = None;
        let mut readme = None;
        let mut license = None;
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            let slot = match argument.to_str() {
                Some("--binary") => &mut binary,
                Some("--target") => &mut target,
                Some("--version") => &mut version,
                Some("--output") => &mut output,
                Some("--archive-name") => &mut archive_name,
                Some("--readme") => &mut readme,
                Some("--license") => &mut license,
                _ => return Err(format!("未知 dist 选项：{}", argument.to_string_lossy()).into()),
            };
            if slot.is_some() {
                return Err(format!("重复的 dist 选项：{}", argument.to_string_lossy()).into());
            }
            let value = arguments.next().ok_or("dist 选项缺少参数")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err(
                    format!("dist 选项缺少有效参数：{}", argument.to_string_lossy()).into(),
                );
            }
            *slot = Some(value.clone());
        }
        let target = target.ok_or("dist 缺少 --target")?;
        let target = Target::parse(target.to_str().ok_or("分发目标必须是 UTF-8")?)?;
        let version = version.ok_or("dist 缺少 --version")?;
        let version = version.into_string().map_err(|_| "分发版本必须是 UTF-8")?;
        let mut parts = version.split('.');
        let valid = (0..3).all(|_| {
            parts.next().is_some_and(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && (part.len() == 1 || !part.starts_with('0'))
            })
        }) && parts.next().is_none();
        if !valid {
            return Err("分发版本必须是无前缀的 major.minor.patch 正式版本".into());
        }
        let archive_name = match archive_name {
            Some(name) => name.into_string().map_err(|_| "归档文件名必须是 UTF-8")?,
            None => target.archive_name(&version),
        };
        validate_archive_name(&archive_name)?;
        let package = root.join("crates/kenvoy");
        Ok(Self {
            binary: PathBuf::from(binary.ok_or("dist 缺少 --binary")?),
            target,
            version,
            output: PathBuf::from(output.ok_or("dist 缺少 --output")?),
            archive_name,
            readme: readme
                .map(PathBuf::from)
                .unwrap_or_else(|| package.join("README.md")),
            license: license
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("LICENSE")),
        })
    }
}

fn validate_archive_name(name: &str) -> Result<()> {
    let valid = name.ends_with(".zip")
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    let stem = name.split('.').next().unwrap_or_default();
    let reserved = ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
        || (stem.len() == 4
            && stem.get(..3).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
            })
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if !valid || reserved {
        return Err("归档文件名必须是普通 .zip 文件名：仅字母、数字、点、短横线和下划线，以字母或数字开头，且不能是 Windows 设备名".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_archive_name;

    #[test]
    fn archive_name_accepts_only_portable_zip_basenames() {
        for name in ["custom.zip", "kenvoy-1.2.3-aarch64-apple-darwin.zip"] {
            assert!(validate_archive_name(name).is_ok(), "{name}");
        }
        for name in [
            "",
            ".zip",
            "../name.zip",
            "dir/name.zip",
            "dir\\name.zip",
            "/name.zip",
            "C:name.zip",
            "name.ZIP",
            "name.tar",
            "name.zip\n",
            "a b.zip",
            "a*.zip",
            "NUL.zip",
            "con.extra.zip",
            "COM1.zip",
            "lpt9.zip",
            "aaé.zip",
        ] {
            assert!(validate_archive_name(name).is_err(), "{name:?}");
        }
    }
}
