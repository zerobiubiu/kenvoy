//! 组合根协调项目、适配器与进程；领域及应用层不依赖这里。
mod build;
mod commands;
mod error;
mod lifecycle;
mod migrations;
mod server;
mod signals;
mod startup;

pub(crate) use commands::execute;
pub(crate) use error::report_diagnostics;

use crate::{
    application::compilation, domain::model::ApplicationIR, infrastructure::project::Project,
};
use error::Result;
use kengine::{AnalysisSession, ProgramIR};

pub(crate) fn compile(
    project: &Project,
    session: &mut AnalysisSession,
) -> Result<(ApplicationIR, ProgramIR)> {
    project.check_compatibility()?;
    let update = project.analyze(session)?;
    Ok(compilation::compile(
        update.program,
        &project.entry,
        &project.config.project.name,
        &crate::infrastructure::javascript::SwcClientCodeTransformer,
    )?)
}
