mod boundaries;
mod fields_options;
mod pages;

use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use kengine::{AnalyzeOptions, Engine};

use crate::{domain::model::ApplicationIR, infrastructure::javascript::SwcClientCodeTransformer};

use super::{AnalyzeResult, analyze};

type TestResult<T = ()> = std::result::Result<T, Box<dyn Error>>;
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new(app: &str, files: &[(&str, &str)]) -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "kenvoy-dsl-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&root)?;
        fs::write(root.join("app.tsx"), app)?;
        for (name, source) in files {
            fs::write(root.join(name), source)?;
        }
        Ok(Self { root })
    }
    fn analyze(&self) -> TestResult<AnalyzeResult> {
        let program = Engine::new().analyze_project(
            &self.root,
            AnalyzeOptions {
                entry_points: vec!["app.tsx".into()],
                ..Default::default()
            },
        )?;
        assert!(!program.has_errors(), "{:?}", program.diagnostics);
        Ok(analyze(
            &program,
            &self.root.join("app.tsx").canonicalize()?,
            "测试应用",
            &SwcClientCodeTransformer,
        ))
    }
    fn valid(&self) -> TestResult<ApplicationIR> {
        let result = self.analyze()?;
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
        Ok(result.application.ok_or("缺少 Application IR")?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
