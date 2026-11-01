//! Cargo 构建选项与测试参数分流。

use crate::Result;
use std::ffi::OsString;

pub(super) fn test_arguments(arguments: &[OsString], ignored: bool) -> Vec<OsString> {
    let mut arguments = arguments.to_vec();
    if ignored {
        if let Some(separator) = arguments.iter().position(|item| item == "--") {
            if !arguments[separator + 1..]
                .iter()
                .any(|item| item == "--ignored")
            {
                arguments.insert(separator + 1, "--ignored".into());
            }
        } else {
            arguments.extend([OsString::from("--"), OsString::from("--ignored")]);
        }
    }
    arguments
}

#[derive(Default)]
pub(super) struct CargoOptions {
    pub(super) build_arguments: Vec<OsString>,
    pub(super) test_arguments: Vec<OsString>,
}

impl CargoOptions {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self> {
        let mut result = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            let argument = &arguments[index];
            match argument.to_str() {
                Some("--offline" | "--locked") => {}
                Some("--release") => result.build_arguments.push(argument.clone()),
                Some("--profile" | "--target-dir" | "--target") => {
                    result.build_arguments.push(argument.clone());
                    index += 1;
                    result
                        .build_arguments
                        .push(arguments.get(index).ok_or("构建选项缺少参数")?.clone());
                }
                Some(value)
                    if ["--profile=", "--target-dir=", "--target="]
                        .iter()
                        .any(|prefix| value.starts_with(prefix)) =>
                {
                    result.build_arguments.push(argument.clone());
                }
                Some("--") => {
                    result.test_arguments.extend_from_slice(&arguments[index..]);
                    break;
                }
                _ => result.test_arguments.push(argument.clone()),
            }
            index += 1;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_options_preserve_build_location_and_test_arguments() -> Result<()> {
        let options = CargoOptions::parse(
            &[
                "--target-dir",
                "alternate target",
                "--release",
                "--offline",
                "--test",
                "http",
                "--",
                "--nocapture",
            ]
            .map(OsString::from),
        )?;
        assert_eq!(
            options.build_arguments,
            ["--target-dir", "alternate target", "--release"].map(OsString::from)
        );
        assert_eq!(
            options.test_arguments,
            ["--test", "http", "--", "--nocapture"].map(OsString::from)
        );
        assert!(CargoOptions::parse(&["--target-dir".into()]).is_err());
        Ok(())
    }

    #[test]
    fn browser_ignored_mode_is_kept_with_explicit_test_separator() {
        assert_eq!(
            test_arguments(&["forms".into(), "--".into(), "--nocapture".into()], true),
            ["forms", "--", "--ignored", "--nocapture"].map(OsString::from)
        );
        assert_eq!(
            test_arguments(&[], true),
            ["--", "--ignored"].map(OsString::from)
        );
    }
}
