//! 开发快照只能指向显式指定、尚不存在且与正式数据隔离的路径。
use std::path::Path;

use super::{
    Project,
    error::ProjectError,
    paths::{overlaps, safe_path},
};

impl Project {
    pub fn use_snapshot(&mut self, relative: &Path) -> super::Result<()> {
        let path = safe_path(&self.root, relative)?;
        let parent = path
            .parent()
            .ok_or_else(|| ProjectError::InvalidConfig("开发快照必须位于独立子目录".into()))?;
        let persistent = self.database.parent().unwrap_or(&self.root);
        let source = self.entry.parent().unwrap_or(&self.root);
        if parent == self.root
            || overlaps(parent, persistent)
            || overlaps(parent, source)
            || overlaps(parent, &self.out_dir)
            || overlaps(parent, &self.root.join("migrations"))
        {
            return Err(ProjectError::InvalidConfig(
                "开发快照必须使用独立于源码、正式数据、迁移及产物的目录，例如 --snapshot dev-snapshots/try.sqlite".into(),
            ));
        }
        for suffix in ["", "-wal", "-shm", ".kenvoy-snapshot.json"] {
            let mut candidate = path.as_os_str().to_owned();
            candidate.push(suffix);
            if Path::new(&candidate)
                .try_exists()
                .map_err(|error| super::error::io(Path::new(&candidate), error))?
            {
                return Err(ProjectError::InvalidConfig(format!(
                    "开发快照要求一次性空路径，拒绝复用或覆盖：{}",
                    Path::new(&candidate).display()
                )));
            }
        }
        self.database = path;
        self.snapshot = true;
        Ok(())
    }
}
