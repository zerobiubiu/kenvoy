use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

use kengine::{DependencyKind, ProgramIR, ResolvedModule};
use serde_json::{Value, json};

use super::DevUpdate;
use crate::{
    domain::model::ApplicationIR,
    infrastructure::{
        client_build::{
            Result,
            cache::{directory, reject_link},
            digest,
            error::{invalid, io},
            graph::{private_modules, verify_sources},
        },
        project::Project,
    },
};

#[cfg(test)]
mod tests;

const MAX_MIRROR_FILES: usize = 8192;
const MAX_MIRROR_FILE: usize = 32 * 1024 * 1024;
const MAX_MIRROR_TOTAL: usize = 256 * 1024 * 1024;

pub(super) fn hashes(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect()
}

pub(super) fn snapshot(
    project: &Project,
    program: &ProgramIR,
    app: &ApplicationIR,
    seed: &Value,
    mirror: &Path,
) -> Result<DevUpdate> {
    verify_sources(program, &project.root)?;
    let mut request = seed.clone();
    let private = private_modules(program, app);
    let mut queue = VecDeque::new();
    for reference in request["references"]
        .as_array()
        .ok_or_else(|| invalid("缺少客户端引用"))?
    {
        if let Some(slice) = reference.get("slice") {
            if let Some(imports) = slice["imports"].as_object() {
                for target in imports.values() {
                    if target["kind"] == "file" {
                        queue.push_back(PathBuf::from(
                            target["module"]
                                .as_str()
                                .ok_or_else(|| invalid("客户端导入路径无效"))?,
                        ));
                    }
                }
            }
        } else if let Some(path) = reference["module"].as_str()
            && Path::new(path).is_absolute()
        {
            queue.push_back(path.into());
        }
    }
    let mut files = BTreeMap::new();
    let mut total = 0;
    let mut seen = BTreeSet::new();
    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        if private.contains(&path)
            || path == project.entry
            || path.to_string_lossy().contains(".diu.")
            || path.to_string_lossy().contains(".server.")
        {
            return Err(invalid("开发镜像拒绝声明入口或服务端私有闭包"));
        }
        let module = program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(|| invalid("开发引用不在已分析源码图中"))?;
        if module.source.len() > remaining_capacity(files.len(), total)? {
            return Err(invalid("开发客户端镜像超过文件数量或大小上限"));
        }
        total += module.source.len();
        files.insert(
            relative(&project.root, &path)?,
            module.source.as_bytes().to_vec(),
        );
        for edge in program.module_graph.edges.iter().filter(|edge| {
            edge.importer == path
                && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
        }) {
            if let Some(ResolvedModule::File { path }) = &edge.target {
                queue.push_back(path.clone());
            }
        }
    }
    let source_dir = project
        .entry
        .parent()
        .ok_or_else(|| invalid("缺少源码目录"))?;
    collect_assets(&project.root, source_dir, &mut files, &mut total)?;
    let map = |path: &str| -> Result<PathBuf> {
        Ok(mirror.join(relative(&project.root, Path::new(path))?))
    };
    for reference in request["references"]
        .as_array_mut()
        .ok_or_else(|| invalid("缺少客户端引用"))?
    {
        if let Some(slice) = reference.get_mut("slice") {
            if let Some(imports) = slice["imports"].as_object_mut() {
                for target in imports.values_mut() {
                    if target["kind"] == "file" {
                        target["module"] = json!(map(target["module"]
                            .as_str()
                            .ok_or_else(|| invalid("客户端导入路径无效"))?)?);
                    }
                }
            }
        } else if let Some(path) = reference["module"].as_str()
            && Path::new(path).is_absolute()
        {
            reference["module"] = json!(map(path)?);
        }
    }
    request["source_modules"] = json!(
        files
            .iter()
            .filter(|(name, _)| is_code(Path::new(name)))
            .map(|(name, bytes)| (mirror.join(name), digest(bytes)))
            .collect::<BTreeMap<_, _>>()
    );
    request["mirror_root"] = json!(mirror);
    Ok(DevUpdate {
        root: project.root.clone(),
        request,
        files,
    })
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    let path = path
        .strip_prefix(root)
        .map_err(|_| invalid("镜像文件越过项目根目录"))?;
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("镜像文件路径无效"));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}
fn is_code(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension,
                "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
            )
        })
}
fn remaining_capacity(files: usize, total: usize) -> Result<usize> {
    if files >= MAX_MIRROR_FILES || total > MAX_MIRROR_TOTAL {
        return Err(invalid("开发客户端镜像超过文件数量或大小上限"));
    }
    Ok((MAX_MIRROR_TOTAL - total).min(MAX_MIRROR_FILE))
}

fn read_bounded(input: impl std::io::Read, limit: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    input.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(std::io::Error::other("开发资产在读取时超过剩余容量上限"));
    }
    Ok(bytes)
}

fn collect_assets(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut usize,
) -> Result<()> {
    for entry in fs::read_dir(path).map_err(|error| io(path, error))? {
        let entry = entry.map_err(|error| io(path, error))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name.contains(".server.") || name == "node_modules" {
            continue;
        }
        let kind = entry.file_type().map_err(|error| io(&path, error))?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_assets(root, &path, files, total)?;
        } else if kind.is_file()
            && path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| {
                    matches!(
                        extension,
                        "css"
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
                })
        {
            let limit = remaining_capacity(files.len(), *total)?;
            if entry.metadata().map_err(|error| io(&path, error))?.len() > limit as u64 {
                return Err(invalid("开发资产超过剩余镜像容量上限"));
            }
            let bytes = read_bounded(
                fs::File::open(&path).map_err(|error| io(&path, error))?,
                limit,
            )
            .map_err(|error| io(&path, error))?;
            *total += bytes.len();
            files.insert(relative(root, &path)?, bytes);
        }
    }
    Ok(())
}

pub(super) fn write_mirror(
    root: &Path,
    mirror: &Path,
    old: &BTreeMap<String, String>,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for name in old.keys().filter(|name| !files.contains_key(*name)) {
        let path = mirror.join(name);
        let canonical = path.canonicalize().map_err(|error| io(&path, error))?;
        if !canonical.starts_with(mirror) {
            return Err(invalid("镜像旧文件链接越界"));
        }
        fs::remove_file(&canonical).map_err(|error| io(canonical, error))?;
    }
    for (name, bytes) in files {
        if old.get(name).is_some_and(|hash| hash == &digest(bytes)) {
            continue;
        }
        let path = mirror.join(name);
        let parent = path.parent().ok_or_else(|| invalid("镜像路径无父目录"))?;
        directory(root, parent)?;
        reject_link(&path)?;
        fs::write(&path, bytes).map_err(|error| io(path, error))?;
    }
    Ok(())
}
