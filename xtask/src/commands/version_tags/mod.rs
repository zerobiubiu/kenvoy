//! 本地版本标签预览与显式创建；不发布、不推送、不改写历史。

mod git;
mod history;
mod manifest;
mod options;
mod plan;

#[cfg(test)]
mod tests;

use crate::Result;
use std::{ffi::OsString, path::Path};

pub(super) fn execute(root: &Path, arguments: &[OsString]) -> Result<()> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h")) {
        println!(
            "cargo xtask version-tags [--apply] [--repo PATH]\n\
             默认只读预览 dev 全部可达历史，只依据已提交的 crates/kenvoy/Cargo.toml。\n\
             --apply 仅在当前 dev 分支和完整非浅仓库创建缺失的 annotated 标签。\n\
             --repo PATH 显式选择工作区根目录，隔离仓库使用其唯一根；默认校验本仓库固定根。\n\
             根标记 v0.0.0（0.0.0 init），同时保留或补齐根 manifest 声明的真实版本标签。\n\
             正式 X.Y.Z 在首次版本变化处标记；已有同版本有效标签保留，不移动、不删除、不推送。\n\
             需要 Git 2.36 或更新版本；冲突会失败，不会自动修复既有标签。"
        );
        return Ok(());
    }
    let options = options::Options::parse(root, arguments)?;
    let repository = git::Repository::open(&options.repository)?;
    if options.apply {
        repository.require_dev()?;
    }
    let expected_root = (!options.custom_repository).then_some(history::INITIAL_ROOT);
    let history = history::scan(&repository, expected_root)?;
    let plan = plan::Plan::build(&history, repository.version_tags()?)?;
    println!("版本标签仓库：{}", repository.path.display());
    for tag in &plan.existing {
        println!("保留 {} -> {}", tag.name, tag.commit);
    }
    for tag in &plan.missing {
        println!("待创建 {} -> {}（{}）", tag.name, tag.commit, tag.message);
    }
    if options.apply {
        repository.create_tags(&history.head, &plan.existing, &plan.missing)?;
        println!(
            "本地版本标签维护完成：新增 {}，保留 {}；未推送、未移动既有标签。",
            plan.missing.len(),
            plan.existing.len()
        );
    } else {
        println!(
            "只读预览：缺失 {} 个标签，未作任何修改；需要创建时在 dev 显式添加 --apply。",
            plan.missing.len()
        );
    }
    Ok(())
}
