//! 按父提交在先的顺序扫描 dev；初始化标记不排除根声明的真实版本。

use super::{git::Repository, manifest::ManifestReader};
use crate::Result;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub(super) const INITIAL_ROOT: &str = "7d3b5c2860d6e3c64948612a53d0df74ddf47883";

pub(super) struct Introduction {
    pub(super) version: Rc<str>,
    pub(super) commit: String,
}

pub(super) struct History {
    pub(super) head: String,
    pub(super) root: String,
    pub(super) versions: HashMap<String, Option<Rc<str>>>,
    pub(super) introductions: Vec<Introduction>,
}

pub(super) fn scan(repository: &Repository, expected_root: Option<&str>) -> Result<History> {
    let head = repository.read(&["rev-parse", "--verify", "refs/heads/dev^{commit}"])?;
    let commits =
        repository.read(&["rev-list", "--reverse", "--topo-order", "--parents", &head])?;
    let mut reader = ManifestReader::start(repository)?;
    let mut versions: HashMap<String, Option<Rc<str>>> = HashMap::new();
    let mut introductions = Vec::new();
    let mut introduced = HashSet::new();
    let mut root = None;
    for line in commits.lines() {
        let mut fields = line.split_whitespace();
        let commit = fields.next().ok_or("Git 提交历史包含空记录")?;
        let is_root = fields.clone().next().is_none();
        let version = reader.version(commit)?;
        let mut inherited = false;
        for parent in fields {
            let parent_version = versions
                .get(parent)
                .ok_or_else(|| format!("提交 {commit} 的父提交 {parent} 不在完整拓扑历史中"))?;
            inherited |= parent_version.as_deref() == version.as_deref();
        }
        if is_root {
            if root.replace(commit.to_owned()).is_some() {
                return Err("dev 历史包含多个根，无法唯一确定 v0.0.0；未创建标签".into());
            }
            if let Some(expected) = expected_root
                && commit != expected
            {
                return Err(format!(
                    "本仓库根提交应为 {expected}，实际为 {commit}；隔离仓库请显式使用 --repo"
                )
                .into());
            }
        }
        if let Some(version) = &version
            && version.as_ref() != "0.0.0"
            && (is_root || !inherited)
            && introduced.insert(Rc::clone(version))
        {
            introductions.push(Introduction {
                version: Rc::clone(version),
                commit: commit.to_owned(),
            });
        }
        versions.insert(commit.to_owned(), version);
    }
    reader.finish()?;
    if versions.get(&head).and_then(Option::as_ref).is_none() {
        return Err("dev 当前已提交的 crates/kenvoy/Cargo.toml 不存在，无法维护版本标签".into());
    }
    Ok(History {
        head,
        root: root.ok_or("dev 没有可确定的根提交")?,
        versions,
        introductions,
    })
}
