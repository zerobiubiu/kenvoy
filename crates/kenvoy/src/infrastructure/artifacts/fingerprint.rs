use std::{collections::BTreeMap, fs, path::Path};

use super::{
    INPUT_HASH_VERSION, MAX_FILE, MAX_TOTAL, Result, digest,
    files::{read_limited, reject_link},
    invalid, io,
};
use crate::infrastructure::project::Project;

/// 比较源码、静态资产、锁文件及 migrations/ 的全部内容（含未固化草稿）；
/// 不读取数据目录、环境密钥、时间戳或构建缓存。
pub fn input_fingerprint(project: &Project) -> Result<String> {
    fn walk(
        project: &Project,
        folder: &Path,
        migrations: &Path,
        persistent_data: &Path,
        files: &mut BTreeMap<String, String>,
        total: &mut u64,
    ) -> Result<()> {
        for entry in fs::read_dir(folder).map_err(|error| io(folder, error))? {
            let entry = entry.map_err(|error| io(folder, error))?;
            let path = entry.path();
            if path == project.out_dir
                || path == persistent_data
                || project.database.parent() == Some(path.as_path())
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(
                name.as_str(),
                ".git" | ".kenvoy" | "node_modules" | "target"
            ) || name.starts_with(".env")
            {
                continue;
            }
            reject_link(&path)?;
            if path.is_dir() {
                walk(project, &path, migrations, persistent_data, files, total)?;
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !path.starts_with(migrations)
                && !matches!(
                    extension,
                    "ts" | "tsx"
                        | "mts"
                        | "cts"
                        | "js"
                        | "jsx"
                        | "mjs"
                        | "cjs"
                        | "css"
                        | "json"
                        | "html"
                        | "toml"
                        | "svg"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "gif"
                        | "webp"
                        | "avif"
                        | "ico"
                        | "woff"
                        | "woff2"
                        | "ttf"
                        | "otf"
                )
            {
                continue;
            }
            if files.len() >= 10000 {
                return Err(invalid("源码输入超过 10000 个文件"));
            }
            let bytes = read_limited(&path, MAX_FILE)?;
            *total += bytes.len() as u64;
            if *total > MAX_TOTAL {
                return Err(invalid("源码输入总大小超过 256 MiB"));
            }
            let relative = path
                .strip_prefix(&project.root)
                .map_err(|_| invalid("源码输入越界"))?
                .to_str()
                .ok_or_else(|| invalid("源码路径必须为 UTF-8"))?
                .replace('\\', "/");
            files.insert(relative, digest(&bytes));
        }
        Ok(())
    }
    let persistent_database = project.root.join(&project.config.database.path);
    let configured_data = persistent_database
        .parent()
        .ok_or_else(|| invalid("配置的数据库路径缺少持久化目录"))?;
    let persistent_data = match configured_data.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => configured_data.to_owned(),
        Err(error) => return Err(io(configured_data, error)),
    };
    let mut files = BTreeMap::new();
    walk(
        project,
        &project.root,
        &project.root.join("migrations"),
        &persistent_data,
        &mut files,
        &mut 0,
    )?;
    Ok(digest(&serde_json::to_vec(&(
        INPUT_HASH_VERSION,
        project.snapshot,
        env!("CARGO_PKG_VERSION"),
        crate::infrastructure::frontend::FRAMEWORK_FINGERPRINT,
        crate::infrastructure::frontend::fingerprint(),
        files,
    ))?))
}
