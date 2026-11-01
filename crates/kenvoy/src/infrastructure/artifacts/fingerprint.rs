use std::{collections::BTreeMap, fs, path::Path};

use super::{
    MAX_FILE, MAX_TOTAL, Result, digest,
    files::{read_limited, reject_link},
    invalid, io,
};
use crate::infrastructure::project::Project;

/// 比较源文件、CSS/静态资产和锁文件；不读取数据目录、环境密钥或构建缓存。
pub fn input_fingerprint(project: &Project) -> Result<String> {
    fn walk(
        project: &Project,
        folder: &Path,
        files: &mut BTreeMap<String, String>,
        total: &mut u64,
    ) -> Result<()> {
        for entry in fs::read_dir(folder).map_err(|error| io(folder, error))? {
            let entry = entry.map_err(|error| io(folder, error))?;
            let path = entry.path();
            if path == project.out_dir || project.database.parent() == Some(path.as_path()) {
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
                walk(project, &path, files, total)?;
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !matches!(
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
            ) {
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
    let mut files = BTreeMap::new();
    walk(project, &project.root, &mut files, &mut 0)?;
    Ok(digest(&serde_json::to_vec(&(
        env!("CARGO_PKG_VERSION"),
        crate::infrastructure::frontend::FRAMEWORK_FINGERPRINT,
        crate::infrastructure::frontend::fingerprint(),
        files,
    ))?))
}
