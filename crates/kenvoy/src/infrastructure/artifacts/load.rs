use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    BUILD_HASH_VERSION, FORMAT_VERSION, INPUT_HASH_VERSION, Loaded, MAX_FILE, Manifest, Result,
    files::{check_directory, collect, read_limited, reject_link, write_new},
    invalid, io, languages_config_hash, valid_hash,
};
use crate::{
    domain::{i18n::I18nBundle, model::ApplicationIR},
    infrastructure::{
        persistence::sqlite::{migration::catalog::Catalog, schema::SCHEMA_HASH_VERSION},
        project::Project,
    },
};

pub fn load(project: &Project) -> Result<Loaded> {
    load_paths(project)
}
pub fn activate(project: &Project, manifest: &Manifest) -> Result<()> {
    activate_paths(project, manifest)
}
fn activate_paths(project: &Project, manifest: &Manifest) -> Result<()> {
    validate_project(project, manifest)?;
    let root = &project.root;
    let out = &project.out_dir;
    let loaded = load_manifest(root, out, manifest.clone(), project.snapshot)?;
    validate_i18n_project(project, &loaded.i18n)?;
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
fn load_paths(project: &Project) -> Result<Loaded> {
    let root = &project.root;
    let out = &project.out_dir;
    check_directory(root, out)?;
    let manifest_path = out.join("manifest.json");
    let bytes = read_limited(&manifest_path, 4 * 1024 * 1024)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    validate_project(project, &manifest)?;
    let loaded = load_manifest(root, out, manifest, project.snapshot)?;
    validate_i18n_project(project, &loaded.i18n)?;
    Ok(loaded)
}
pub(super) fn load_manifest(
    root: &Path,
    out: &Path,
    manifest: Manifest,
    development_snapshot: bool,
) -> Result<Loaded> {
    check_directory(root, out)?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(invalid(
            "产物格式版本不匹配，当前只接受 v4；请从源码重新构建",
        ));
    }
    if manifest.input_hash_version != INPUT_HASH_VERSION
        || manifest.build_hash_version != BUILD_HASH_VERSION
        || manifest.schema_hash_version != SCHEMA_HASH_VERSION
    {
        return Err(invalid(
            "产物输入、构建或 Schema 哈希算法版本不受支持，请重新构建",
        ));
    }
    if manifest.framework_version != env!("CARGO_PKG_VERSION")
        || manifest.framework_fingerprint != crate::infrastructure::frontend::FRAMEWORK_FINGERPRINT
    {
        return Err(invalid(
            "产物框架版本或框架指纹与当前 CLI 不匹配，请重新构建",
        ));
    }
    if !valid_hash(&manifest.build_id)
        || !valid_hash(&manifest.input_fingerprint)
        || !valid_hash(&manifest.languages_config_hash)
        || manifest.identity()? != manifest.build_id
    {
        return Err(invalid("产物内容标识或输入指纹不匹配，请重新构建"));
    }
    if manifest.development_snapshot && !development_snapshot {
        return Err(invalid(
            "开发快照产物不能用于正式运行；请固化迁移并重新构建正式产物",
        ));
    }
    let version = out.join("versions").join(&manifest.build_id);
    check_directory(root, &version)?;
    if manifest.files != collect(&version)? {
        return Err(invalid("产物文件清单、大小或 SHA-256 不匹配"));
    }
    for required in [
        "application.ir",
        "migrations.json",
        "i18n.json",
        "modules/server.json",
        "client/index.html",
    ] {
        if !manifest.files.contains_key(required) {
            return Err(invalid(format!("缺少产物 {required}")));
        }
    }
    let application: ApplicationIR =
        serde_json::from_slice(&read_limited(&version.join("application.ir"), MAX_FILE)?)?;
    let migrations: Catalog =
        serde_json::from_slice(&read_limited(&version.join("migrations.json"), MAX_FILE)?)?;
    migrations.validate()?;
    if !manifest.development_snapshot {
        migrations.validate_target(&application)?;
    }
    let i18n: I18nBundle =
        serde_json::from_slice(&read_limited(&version.join("i18n.json"), MAX_FILE)?)?;
    i18n.validate()
        .map_err(|error| invalid(error.to_string()))?;
    let server_modules = read_limited(&version.join("modules/server.json"), MAX_FILE)?;
    Ok(Loaded {
        manifest,
        application,
        migrations,
        i18n: Arc::new(i18n),
        server_modules,
        client: version.join("client"),
    })
}

fn validate_project(project: &Project, manifest: &Manifest) -> Result<()> {
    let configured = &project.config.project;
    if manifest.project_name != configured.name
        || manifest.project_version != configured.version
        || Some(manifest.min_cli_version.as_str()) != configured.min_cli_version.as_deref()
        || Some(manifest.project_format_version.as_str()) != configured.format_version.as_deref()
        || manifest.languages_config_hash != languages_config_hash(project)?
    {
        return Err(invalid(
            "产物的项目身份、兼容要求或 languages 配置与 kenvoy.toml 不一致；不能替换配置冒充新版本，请从对应源码重新构建",
        ));
    }
    Ok(())
}

fn validate_i18n_project(project: &Project, bundle: &I18nBundle) -> Result<()> {
    let current = crate::infrastructure::project::i18n::load(project)
        .map_err(|error| invalid(error.to_string()))?;
    if &current != bundle {
        return Err(invalid(
            "根目录 translations.json 或 languages 配置与产物词典不一致，请从对应源码重新构建；无源码运行同样必须保留匹配的 translations.json",
        ));
    }
    Ok(())
}
