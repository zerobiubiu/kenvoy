//! 批量读取 Git 对象并按 blob 缓存版本，不读取工作区或暂存区。

use super::git::Repository;
use crate::Result;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fmt::Arguments,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    rc::Rc,
};

pub(super) const MANIFEST_PATH: &str = "crates/kenvoy/Cargo.toml";

pub(super) fn is_release_version(value: &str) -> bool {
    let mut parts = value.split('.');
    for _ in 0..3 {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.is_empty()
            || !part.bytes().all(|byte| byte.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return false;
        }
    }
    parts.next().is_none()
}

#[derive(Deserialize)]
struct Manifest {
    package: Package,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
}

pub(super) fn parse_manifest(bytes: &[u8]) -> Result<Rc<str>> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("manifest 不是 UTF-8：{error}"))?;
    let manifest: Manifest =
        toml::from_str(text).map_err(|error| format!("无法解析 Kenvoy Cargo manifest：{error}"))?;
    if manifest.package.name != "kenvoy" {
        return Err(format!(
            "{MANIFEST_PATH} 的 package.name 必须是 kenvoy，实际为 {:?}",
            manifest.package.name
        )
        .into());
    }
    if !is_release_version(&manifest.package.version) {
        return Err(format!(
            "Kenvoy 版本必须是无前导零的正式 X.Y.Z，不接受预发布、构建元数据或继承版本：{:?}",
            manifest.package.version
        )
        .into());
    }
    Ok(Rc::from(manifest.package.version))
}

pub(super) struct ManifestReader {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    header: String,
    contents: Vec<u8>,
    versions: HashMap<String, Rc<str>>,
}

impl ManifestReader {
    pub(super) fn start(repository: &Repository) -> Result<Self> {
        let mut child = repository
            .command()
            .args(["cat-file", "--batch-command"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("无法启动 Git manifest 对象读取器：{error}"))?;
        let input = child.stdin.take().ok_or("Git 对象读取器缺少标准输入")?;
        let output = child.stdout.take().ok_or("Git 对象读取器缺少标准输出")?;
        Ok(Self {
            child,
            input: Some(input),
            output: BufReader::new(output),
            header: String::new(),
            contents: Vec::new(),
            versions: HashMap::new(),
        })
    }

    pub(super) fn version(&mut self, commit: &str) -> Result<Option<Rc<str>>> {
        self.request(format_args!("info {commit}:{MANIFEST_PATH}"))?;
        if self.header.trim_end().ends_with(" missing") {
            return Ok(None);
        }
        let (object, size) = self.object_header()?;
        if let Some(version) = self.versions.get(object) {
            return Ok(Some(Rc::clone(version)));
        }
        let object = object.to_owned();
        self.request(format_args!("contents {object}"))?;
        let (returned_object, returned_size) = self.object_header()?;
        if returned_object != object || returned_size != size {
            return Err("Git manifest 对象信息在读取期间发生变化".into());
        }
        self.contents.resize(size, 0);
        self.output.read_exact(&mut self.contents)?;
        let mut separator = [0];
        self.output.read_exact(&mut separator)?;
        if separator != [b'\n'] {
            return Err("Git manifest 对象缺少内容分隔符".into());
        }
        let version = parse_manifest(&self.contents)
            .map_err(|error| format!("提交 {commit} 的 {MANIFEST_PATH} 无效：{error}"))?;
        self.versions.insert(object, Rc::clone(&version));
        Ok(Some(version))
    }

    fn request(&mut self, request: Arguments<'_>) -> Result<()> {
        let input = self.input.as_mut().ok_or("Git 对象读取器已经关闭")?;
        writeln!(input, "{request}")?;
        input.flush()?;
        self.header.clear();
        if self.output.read_line(&mut self.header)? == 0 {
            return Err("Git 对象读取器提前退出；需要 Git 2.36 或更新版本及完整本地对象".into());
        }
        Ok(())
    }

    fn object_header(&self) -> Result<(&str, usize)> {
        let mut fields = self.header.split_whitespace();
        let (Some(object), Some("blob"), Some(size), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(
                format!("Git manifest 对象不是可读取的 blob：{}", self.header.trim()).into(),
            );
        };
        let size = size
            .parse()
            .map_err(|error| format!("Git manifest 对象长度无效：{error}"))?;
        Ok((object, size))
    }

    pub(super) fn finish(mut self) -> Result<()> {
        drop(self.input.take());
        let status = self.child.wait()?;
        if !status.success() {
            return Err(format!("Git manifest 对象读取器退出：{status}").into());
        }
        Ok(())
    }
}

impl Drop for ManifestReader {
    fn drop(&mut self) {
        drop(self.input.take());
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
