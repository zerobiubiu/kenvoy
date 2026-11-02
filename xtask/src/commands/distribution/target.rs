//! 六个发布目标与可执行格式之间的固定对应关系。

use crate::Result;

#[derive(Clone, Copy)]
pub(super) enum Platform {
    Windows,
    Linux,
    Macos,
}

#[derive(Clone, Copy)]
pub(super) struct Target {
    pub(super) triple: &'static str,
    /// 分发文件名中的架构标识：x86_64 或 ARM64（不使用 aarch64）。
    pub(super) file_arch: &'static str,
    pub(super) platform: Platform,
    pub(super) arm64: bool,
}

impl Target {
    pub(super) fn parse(value: &str) -> Result<Self> {
        let (triple, file_arch, platform, arm64) = match value {
            "x86_64-pc-windows-msvc" => {
                ("x86_64-pc-windows-msvc", "x86_64", Platform::Windows, false)
            }
            "aarch64-pc-windows-msvc" => {
                ("aarch64-pc-windows-msvc", "ARM64", Platform::Windows, true)
            }
            "x86_64-unknown-linux-musl" => (
                "x86_64-unknown-linux-musl",
                "x86_64",
                Platform::Linux,
                false,
            ),
            "aarch64-unknown-linux-musl" => {
                ("aarch64-unknown-linux-musl", "ARM64", Platform::Linux, true)
            }
            "x86_64-apple-darwin" => ("x86_64-apple-darwin", "x86_64", Platform::Macos, false),
            "aarch64-apple-darwin" => ("aarch64-apple-darwin", "ARM64", Platform::Macos, true),
            _ => return Err(format!("不支持的分发目标：{value}").into()),
        };
        Ok(Self {
            triple,
            file_arch,
            platform,
            arm64,
        })
    }

    /// 分发归档名：kenvoy-<版本>-<平台>-<架构>.zip；ARM64 架构使用 ARM64 而非 aarch64。
    pub(super) fn archive_name(self, version: &str) -> String {
        format!(
            "kenvoy-{version}-{}-{}.zip",
            self.platform_file_name(),
            self.file_arch
        )
    }

    /// 分发文件名中的平台标识：windows / linux / macos。
    pub(super) fn platform_file_name(self) -> &'static str {
        match self.platform {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
            Platform::Macos => "macos",
        }
    }

    pub(super) fn binary_name(self) -> &'static str {
        match self.platform {
            Platform::Windows => "kenvoy.exe",
            Platform::Linux | Platform::Macos => "kenvoy",
        }
    }

    pub(super) fn host_compatible(self) -> bool {
        let architecture = if self.arm64 { "aarch64" } else { "x86_64" };
        let operating_system = match self.platform {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
            Platform::Macos => "macos",
        };
        std::env::consts::ARCH == architecture && std::env::consts::OS == operating_system
    }
}
