//! 先验证所有既有版本标签，再列出缺失标签；从不移动历史标签。

use super::history::History;
use crate::Result;
use std::collections::HashSet;

pub(super) struct ExistingTag {
    pub(super) name: String,
    pub(super) object: String,
    pub(super) commit: String,
}

pub(super) struct NewTag {
    pub(super) name: String,
    pub(super) commit: String,
    pub(super) message: String,
}

pub(super) struct Plan {
    pub(super) existing: Vec<ExistingTag>,
    pub(super) missing: Vec<NewTag>,
}

impl Plan {
    pub(super) fn build(history: &History, existing: Vec<ExistingTag>) -> Result<Self> {
        for tag in &existing {
            if tag.name == "v0.0.0" {
                if tag.commit != history.root {
                    return Err(format!(
                        "标签 v0.0.0 必须指向根 {}，实际指向 {}；不会移动既有标签",
                        history.root, tag.commit
                    )
                    .into());
                }
                continue;
            }
            if tag.commit == history.root {
                return Err(format!(
                    "标签 {} 指向根提交；根仅保留 v0.0.0，不按根的旧 manifest 版本补标签",
                    tag.name
                )
                .into());
            }
            let version = history.versions.get(&tag.commit).ok_or_else(|| {
                format!("标签 {} 的目标 {} 不在 dev 历史中", tag.name, tag.commit)
            })?;
            if version.as_deref() != tag.name.strip_prefix('v') {
                return Err(format!(
                    "标签 {} 的目标 {} 声明版本为 {:?}，与标签冲突；不会覆盖",
                    tag.name,
                    tag.commit,
                    version.as_deref()
                )
                .into());
            }
        }
        let names: HashSet<&str> = existing.iter().map(|tag| tag.name.as_str()).collect();
        let mut missing = Vec::new();
        if !names.contains("v0.0.0") {
            missing.push(NewTag {
                name: "v0.0.0".to_owned(),
                commit: history.root.clone(),
                message: "0.0.0 init".to_owned(),
            });
        }
        for introduction in &history.introductions {
            let name = format!("v{}", introduction.version);
            if !names.contains(name.as_str()) {
                missing.push(NewTag {
                    name,
                    commit: introduction.commit.clone(),
                    message: format!("{} 版本", introduction.version),
                });
            }
        }
        Ok(Self { existing, missing })
    }
}
