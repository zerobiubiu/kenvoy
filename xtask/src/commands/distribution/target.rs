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
    pub(super) platform: Platform,
    pub(super) arm64: bool,
}

impl Target {
    pub(super) fn parse(value: &str) -> Result<Self> {
        let (triple, platform, arm64) = match value {
            "x86_64-pc-windows-msvc" => ("x86_64-pc-windows-msvc", Platform::Windows, false),
            "aarch64-pc-windows-msvc" => ("aarch64-pc-windows-msvc", Platform::Windows, true),
            "x86_64-unknown-linux-musl" => ("x86_64-unknown-linux-musl", Platform::Linux, false),
            "aarch64-unknown-linux-musl" => ("aarch64-unknown-linux-musl", Platform::Linux, true),
            "x86_64-apple-darwin" => ("x86_64-apple-darwin", Platform::Macos, false),
            "aarch64-apple-darwin" => ("aarch64-apple-darwin", Platform::Macos, true),
            _ => return Err(format!("不支持的分发目标：{value}").into()),
        };
        Ok(Self {
            triple,
            platform,
            arm64,
        })
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
