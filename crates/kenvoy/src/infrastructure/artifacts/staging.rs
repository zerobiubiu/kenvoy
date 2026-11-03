use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    BUILD_HASH_VERSION, FORMAT_VERSION, INPUT_HASH_VERSION, Loaded, Manifest, Result,
    files::{collect, directory, reject_link, write_new},
    invalid, io, languages_config_hash,
    load::load_manifest,
    relocate::relocate,
    valid_hash,
};
use crate::{
    domain::{i18n::I18nBundle, model::ApplicationIR},
    infrastructure::{
        persistence::sqlite::{migration::catalog, schema::SCHEMA_HASH_VERSION},
        project::Project,
    },
};

/// OS 文件锁随进程退出释放；不会留下阻止恢复的 PID 锁。
pub struct Staging {
    directory: PathBuf,
    root: PathBuf,
    out_dir: PathBuf,
    project_name: String,
    project_version: String,
    min_cli_version: String,
    project_format_version: String,
    development_snapshot: bool,
    migrations: catalog::Catalog,
    i18n: I18nBundle,
    languages_config_hash: String,
    _lock: fs::File,
}
impl Staging {
    pub fn new(project: &Project) -> Result<Self> {
        let root = project
            .root
            .canonicalize()
            .map_err(|error| io(&project.root, error))?;
        let min_cli_version = project
            .config
            .project
            .min_cli_version
            .clone()
            .ok_or_else(|| invalid("缺少 project.min_cli_version；请先完成项目兼容性升级"))?;
        let project_format_version = project
            .config
            .project
            .format_version
            .clone()
            .ok_or_else(|| invalid("缺少 project.format_version；请先完成项目兼容性升级"))?;
        let migrations = catalog::read(&root)?;
        migrations.validate()?;
        let i18n = crate::infrastructure::project::i18n::load(project)
            .map_err(|error| invalid(error.to_string()))?;
        let languages_config_hash = languages_config_hash(project)?;
        let cache = directory(&root, &root.join(".kenvoy/artifacts"))?;
        let lock_path = cache.join("build.lock");
        reject_link(&lock_path)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io(&lock_path, error))?;
        lock.try_lock()
            .map_err(|_| invalid("另一个构建正在执行，请等待该进程完成"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("系统时间无效"))?
            .as_nanos();
        let stage = cache.join(format!("staging-{}-{stamp}", std::process::id()));
        fs::create_dir(&stage).map_err(|error| io(&stage, error))?;
        fs::create_dir(stage.join("client")).map_err(|error| io(&stage, error))?;
        Ok(Self {
            directory: stage,
            root,
            out_dir: project.out_dir.clone(),
            project_name: project.config.project.name.clone(),
            project_version: project.config.project.version.clone(),
            min_cli_version,
            project_format_version,
            development_snapshot: project.snapshot,
            migrations,
            i18n,
            languages_config_hash,
            _lock: lock,
        })
    }
    pub fn client(&self) -> PathBuf {
        self.directory.join("client")
    }
    /// 开发候选先封存版本，只有数据库和运行态发布成功后才激活清单。
    pub fn seal(
        self,
        application: &ApplicationIR,
        server_modules: &[u8],
        input_fingerprint: String,
        dependencies: BTreeMap<String, String>,
    ) -> Result<Loaded> {
        self.finish(application, server_modules, input_fingerprint, dependencies)
    }
    fn finish(
        mut self,
        application: &ApplicationIR,
        server_modules: &[u8],
        input_fingerprint: String,
        dependencies: BTreeMap<String, String>,
    ) -> Result<Loaded> {
        if !valid_hash(&input_fingerprint) {
            return Err(invalid("输入指纹不是 SHA-256"));
        }
        if !self.development_snapshot {
            self.migrations.validate_target(application)?;
        }
        let mut application = application.clone();
        relocate(&mut application, &self.root)?;
        write_new(
            &self.directory.join("application.ir"),
            &serde_json::to_vec(&application)?,
        )?;
        write_new(
            &self.directory.join("migrations.json"),
            &serde_json::to_vec(&self.migrations)?,
        )?;
        write_new(
            &self.directory.join("i18n.json"),
            &serde_json::to_vec(&self.i18n)?,
        )?;
        fs::create_dir(self.directory.join("modules"))
            .map_err(|error| io(&self.directory, error))?;
        write_new(&self.directory.join("modules/server.json"), server_modules)?;
        let files = collect(&self.directory)?;
        if !files.contains_key("client/index.html")
            || !files
                .keys()
                .any(|name| name.starts_with("client/assets/") && name.ends_with(".js"))
        {
            return Err(invalid("客户端产物缺少入口或真实 JavaScript 资源"));
        }
        let mut manifest = Manifest {
            format_version: FORMAT_VERSION,
            project_name: std::mem::take(&mut self.project_name),
            project_version: std::mem::take(&mut self.project_version),
            min_cli_version: std::mem::take(&mut self.min_cli_version),
            project_format_version: std::mem::take(&mut self.project_format_version),
            development_snapshot: self.development_snapshot,
            input_hash_version: INPUT_HASH_VERSION.into(),
            build_hash_version: BUILD_HASH_VERSION.into(),
            schema_hash_version: SCHEMA_HASH_VERSION.into(),
            framework_version: env!("CARGO_PKG_VERSION").into(),
            framework_fingerprint: crate::infrastructure::frontend::FRAMEWORK_FINGERPRINT.into(),
            build_id: String::new(),
            input_fingerprint,
            languages_config_hash: std::mem::take(&mut self.languages_config_hash),
            files,
            dependencies,
        };
        manifest.build_id = manifest.identity()?;
        let out = directory(&self.root, &self.out_dir)?;
        let owner = out.join(".kenvoy-artifacts");
        // 此标记只证明目录归属；Manifest 版本独立校验，升级后仍保留只追加的旧版本。
        reject_link(&owner)?;
        if owner.exists() {
            if fs::read(&owner).map_err(|error| io(&owner, error))? != b"kenvoy-artifacts-v1\n" {
                return Err(invalid("输出目录不属于当前构建格式"));
            }
        } else {
            if fs::read_dir(&out)
                .map_err(|error| io(&out, error))?
                .next()
                .is_some()
            {
                return Err(invalid(
                    "输出目录包含非 Kenvoy 文件，拒绝覆盖；请选择空 out_dir",
                ));
            }
            write_new(&owner, b"kenvoy-artifacts-v1\n")?;
        }
        let versions = directory(&self.root, &out.join("versions"))?;
        let target = versions.join(&manifest.build_id);
        reject_link(&target)?;
        if target.exists() {
            if collect(&target)? != manifest.files {
                return Err(invalid("同指纹版本内容已改变，保留现场并拒绝覆盖"));
            }
        } else {
            fs::rename(&self.directory, &target).map_err(|error| io(&target, error))?;
        }
        load_manifest(&self.root, &out, manifest, self.development_snapshot)
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        // 只删除本次独占创建且仍在原父目录的 staging；已发布版本永不自动清理。
        if self.directory.exists()
            && self.directory.canonicalize().is_ok_and(|path| {
                path == self.directory
                    && path.parent() == Some(self.root.join(".kenvoy/artifacts").as_path())
            })
        {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}
