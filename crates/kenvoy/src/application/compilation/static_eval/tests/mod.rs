mod evaluation;
mod references;
mod restrictions;

use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use kengine::{AnalyzeOptions, Engine, ProgramIR, ResolveOptions};

use super::{StaticElement, StaticNode};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
struct Project(PathBuf);
impl Project {
    fn new(files: &[(&str, &str)]) -> TestResult<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let project = Self(
            std::env::temp_dir().join(format!("kenvoy-static-{}-{stamp}", std::process::id())),
        );
        fs::create_dir(&project.0)?;
        for (name, source) in files {
            fs::write(project.0.join(name), source)?;
        }
        Ok(project)
    }
    fn program(&self) -> TestResult<ProgramIR> {
        let program = Engine::new().analyze_project(
            &self.0,
            AnalyzeOptions {
                entry_points: vec!["app.tsx".into()],
                resolve: ResolveOptions {
                    externals: ["react".into()].into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        assert!(
            !program.has_errors(),
            "通用解析诊断：{:?}",
            program.diagnostics
        );
        Ok(program)
    }
    fn entry(&self) -> TestResult<PathBuf> {
        Ok(self.0.join("app.tsx").canonicalize()?)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn element(node: &StaticNode) -> &StaticElement {
    match node {
        StaticNode::Element(element) => element,
        _ => panic!("预期 Element"),
    }
}
