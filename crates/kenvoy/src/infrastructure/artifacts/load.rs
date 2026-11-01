use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    FORMAT_VERSION, Loaded, MAX_FILE, Manifest, Result,
    files::{check_directory, collect, read_limited, reject_link, write_new},
    invalid, io, valid_hash,
};
use crate::{domain::model::ApplicationIR, infrastructure::project::Project};

pub fn load(project: &Project) -> Result<Loaded> {
    load_paths(&project.root, &project.out_dir)
}
pub fn activate(project: &Project, manifest: &Manifest) -> Result<()> {
    activate_paths(&project.root, &project.out_dir, manifest)
}
fn activate_paths(root: &Path, out: &Path, manifest: &Manifest) -> Result<()> {
    load_manifest(root, out, manifest.clone())?;
    let encoded = serde_json::to_vec_pretty(manifest)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("系统时间无效"))?
        .as_nanos();
    let temp = out.join(format!(".manifest-{}-{stamp}.tmp", std::process::id()));
    write_new(&temp, &encoded)?;
    let destination = out.join("manifest.json");
    reject_link(&destination)?;
    fs::rename(&temp, &destination).map_err(|error| io(&destination, error))
}
fn load_paths(root: &Path, out: &Path) -> Result<Loaded> {
    check_directory(root, out)?;
    let manifest_path = out.join("manifest.json");
    let bytes = read_limited(&manifest_path, 4 * 1024 * 1024)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    load_manifest(root, out, manifest)
}
pub(super) fn load_manifest(root: &Path, out: &Path, manifest: Manifest) -> Result<Loaded> {
    check_directory(root, out)?;
    if manifest.format_version != FORMAT_VERSION
        || manifest.framework_version != env!("CARGO_PKG_VERSION")
        || manifest.framework_fingerprint != crate::infrastructure::frontend::FRAMEWORK_FINGERPRINT
        || !valid_hash(&manifest.build_id)
        || !valid_hash(&manifest.input_fingerprint)
        || manifest.identity()? != manifest.build_id
    {
        return Err(invalid("清单版本、框架版本或内容标识不匹配，请重新构建"));
    }
    let version = out.join("versions").join(&manifest.build_id);
    check_directory(root, &version)?;
    if manifest.files != collect(&version)? {
        return Err(invalid("产物文件清单、大小或 SHA-256 不匹配"));
    }
    for required in ["application.ir", "modules/server.json", "client/index.html"] {
        if !manifest.files.contains_key(required) {
            return Err(invalid(format!("缺少产物 {required}")));
        }
    }
    let application: ApplicationIR =
        serde_json::from_slice(&read_limited(&version.join("application.ir"), MAX_FILE)?)?;
    let server_modules = read_limited(&version.join("modules/server.json"), MAX_FILE)?;
    Ok(Loaded {
        manifest,
        application,
        server_modules,
        client: version.join("client"),
    })
}
