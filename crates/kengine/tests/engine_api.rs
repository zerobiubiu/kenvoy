//! 正式 KEngine API 的真实语言、结构、文件系统和诊断测试。

#[path = "engine_api/diagnostics.rs"]
mod diagnostics;
#[path = "engine_api/language.rs"]
mod language;
#[path = "engine_api/resolution.rs"]
mod resolution;
#[path = "engine_api/syntax.rs"]
mod syntax;

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use kengine::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Project {
    path: PathBuf,
}

impl Project {
    fn new() -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("kengine-api-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn write(&self, path: &str, source: &str) -> TestResult<PathBuf> {
        let path = self.path.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, source)?;
        Ok(path)
    }

    fn analyze(&self, entry: &str) -> TestResult<ProgramIR> {
        Ok(Engine::new().analyze_project(
            &self.path,
            AnalyzeOptions {
                entry_points: vec![entry.into()],
                ..Default::default()
            },
        )?)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // 仅清理本测试创建的临时目录，不操作项目工作区或外部模块目录。
        if self.path.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn parse(path: &str, source: &str) -> TestResult<ModuleIR> {
    let output = Engine::new().parse_source(Path::new(path), source, ParseOptions::default())?;
    assert!(!output.has_errors(), "实际诊断：{:?}", output.diagnostics);
    output.module.ok_or_else(|| "应返回已解析模块".into())
}

fn element<'a>(module: &'a ModuleIR, name: &str) -> TestResult<&'a ElementIR> {
    module
        .nodes
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Element(value) if value.name.text() == name => Some(value.as_ref()),
            _ => None,
        })
        .ok_or_else(|| format!("缺少元素 {name}").into())
}
