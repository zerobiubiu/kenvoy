use super::*;
use crate::infrastructure::project::Project;
use std::{
    collections::BTreeMap,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn compile(
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

#[test]
fn immutable_versions_detect_tampering_and_preserve_data()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!("kenvoy-artifact-{}-{stamp}", std::process::id()));
    crate::infrastructure::project::create(&path)?;
    let project = Project::load(&path)?;
    let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (application, _) = compile(&project, &mut kengine::AnalysisSession::new())?;
        let before = input_fingerprint(&project)?;
        fs::write(&project.database, "用户数据保护夹具".as_bytes())?;
        assert_eq!(before, input_fingerprint(&project)?);
        let stage = Staging::new(&project)?;
        assert!(Staging::new(&project).is_err());
        crate::infrastructure::frontend::export(&stage.client())?;
        let loaded = stage.seal(&application, b"{}", before.clone(), BTreeMap::new())?;
        assert!(!project.out_dir.join("manifest.json").exists());
        activate(&project, &loaded.manifest)?;
        assert!(loaded.client.join("index.html").is_file());
        assert_eq!(load(&project)?.manifest, loaded.manifest);
        assert_eq!(loaded.server_modules, b"{}");
        assert!(!loaded.application.resources[0].source.path.is_absolute());
        assert_eq!(before, input_fingerprint(&project)?);
        let stage = Staging::new(&project)?;
        crate::infrastructure::frontend::export(&stage.client())?;
        assert_eq!(
            stage
                .seal(&application, b"{}", before, BTreeMap::new())?
                .manifest,
            loaded.manifest
        );
        fs::write(loaded.client.join("index.html"), "用户修改")?;
        assert!(load(&project).is_err());
        assert_eq!(fs::read(&project.database)?, "用户数据保护夹具".as_bytes());
        Ok(())
    })();
    if path.parent() == Some(std::env::temp_dir().as_path()) {
        fs::remove_dir_all(&path)?;
    }
    result
}

#[test]
fn failed_build_keeps_unmanaged_output_and_source_changes_change_fingerprint()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "kenvoy-artifact-unmanaged-{}-{stamp}",
        std::process::id()
    ));
    crate::infrastructure::project::create(&path)?;
    let project = Project::load(&path)?;
    let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (application, _) = compile(&project, &mut kengine::AnalysisSession::new())?;
        let initial = input_fingerprint(&project)?;
        fs::write(project.root.join(".env"), "SECRET=not-for-client")?;
        assert_eq!(initial, input_fingerprint(&project)?);
        fs::write(project.root.join("src/theme.css"), "body { color: red; }")?;
        assert_ne!(initial, input_fingerprint(&project)?);
        for extension in ["mts", "cts", "avif", "ico", "ttf", "otf"] {
            let asset = project.root.join(format!("src/input.{extension}"));
            fs::write(&asset, "变更前")?;
            let previous = input_fingerprint(&project)?;
            fs::write(&asset, "变更后")?;
            assert_ne!(
                previous,
                input_fingerprint(&project)?,
                "{extension} 变更必须使产物失效"
            );
        }
        fs::create_dir(&project.out_dir)?;
        fs::write(project.out_dir.join("user.txt"), "保留已有输出")?;
        let stage = Staging::new(&project)?;
        crate::infrastructure::frontend::export(&stage.client())?;
        assert!(
            stage
                .seal(&application, b"{}", initial, BTreeMap::new())
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(project.out_dir.join("user.txt"))?,
            "保留已有输出"
        );
        assert!(!project.out_dir.join("manifest.json").exists());
        assert!(!project.database.exists());
        Ok(())
    })();
    if path.parent() == Some(std::env::temp_dir().as_path()) {
        fs::remove_dir_all(&path)?;
    }
    result
}
