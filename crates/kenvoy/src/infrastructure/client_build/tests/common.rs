use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::infrastructure::{
    client_build::{Result, cache::directory},
    project::Project,
};

pub(super) type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

pub(super) struct Temporary {
    path: PathBuf,
}
impl Temporary {
    pub(super) fn new() -> std::io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("kenvoy-client-test-{}-{stamp}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if let (Ok(path), Ok(root)) = (
            self.path.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) && path.starts_with(root)
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("kenvoy-client-test-"))
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub(super) fn fixture(
    source: &str,
) -> std::result::Result<(Temporary, Project), Box<dyn std::error::Error>> {
    let temporary = Temporary::new()?;
    fs::create_dir(temporary.path().join("src"))?;
    fs::write(
        temporary.path().join("kenvoy.toml"),
        "[project]\nname=\"client-test\"\n",
    )?;
    fs::write(temporary.path().join("src/app.tsx"), source)?;
    let project = Project::load(temporary.path())?;
    Ok((temporary, project))
}

pub(super) fn stage(project: &Project, name: &str) -> Result<PathBuf> {
    directory(
        &project.root,
        &project.root.join(".kenvoy").join(name).join("client"),
    )
}

pub(super) fn compile(
    project: &Project,
    session: &mut kengine::AnalysisSession,
) -> std::result::Result<
    (crate::domain::model::ApplicationIR, kengine::ProgramIR),
    Box<dyn std::error::Error>,
> {
    let update = project.analyze(session)?;
    Ok(crate::application::compilation::compile(
        update.program,
        &project.entry,
        &project.config.project.name,
        &crate::infrastructure::javascript::SwcClientCodeTransformer,
    )?)
}
