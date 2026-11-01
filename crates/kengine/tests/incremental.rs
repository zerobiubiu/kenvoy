//! 真实文件变更、缓存失效与完整分析等价性测试。

use kengine::*;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

struct Project(PathBuf);
impl Project {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kengine-incremental-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, source: &str) -> Result<PathBuf> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?
        }
        fs::write(&path, source)?;
        Ok(path.canonicalize()?)
    }
    fn options(&self) -> AnalyzeOptions {
        AnalyzeOptions {
            entry_points: vec!["main.ts".into()],
            ..Default::default()
        }
    }
    fn update(
        &self,
        session: &mut AnalysisSession,
        options: AnalyzeOptions,
    ) -> Result<AnalysisUpdate> {
        let update = session.analyze_project(&self.0, options.clone())?;
        let full = Engine::new().analyze_project(&self.0, options)?;
        assert_eq!(update.program, full, "增量与完整分析必须完全一致");
        Ok(update)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn unchanged_and_changed_sources_have_measured_parse_counts() -> Result {
    let project = Project::new()?;
    let main = project.write(
        "main.ts",
        "import { value } from './nested'; export const result = value;",
    )?;
    let nested = project.write("nested.ts", "export { value } from './leaf';")?;
    let leaf = project.write("leaf.ts", "export const value = 1;")?;
    let mut session = AnalysisSession::new();
    let first = project.update(&mut session, project.options())?;
    assert!(
        !first.program.has_errors(),
        "{:?}",
        first.program.diagnostics
    );
    assert_eq!(first.stats.parsed.len(), 3);
    assert!(first.stats.reused.is_empty());
    let second = project.update(&mut session, project.options())?;
    assert!(second.stats.parsed.is_empty());
    assert_eq!(second.stats.reused.len(), 3);
    assert!(second.stats.affected.is_empty());
    project.write("leaf.ts", "export const value = 2;")?;
    let changed = project.update(&mut session, project.options())?;
    assert_eq!(changed.stats.parsed, vec![leaf.clone()]);
    assert_eq!(changed.stats.reused.len(), 2);
    let mut expected = vec![main, nested, leaf];
    expected.sort();
    assert_eq!(changed.stats.affected, expected);
    assert_eq!(session.last_successful(), Some(&changed.program));
    Ok(())
}

#[test]
fn invalid_candidates_are_cached_but_never_replace_last_successful() -> Result {
    let project = Project::new()?;
    project.write(
        "main.ts",
        "import { value } from './leaf'; export const result = value;",
    )?;
    project.write("leaf.ts", "export const value = 1;")?;
    let mut session = AnalysisSession::new();
    let valid = project.update(&mut session, project.options())?.program;
    project.write("leaf.ts", "export const value = ;")?;
    let invalid = project.update(&mut session, project.options())?;
    assert!(invalid.program.has_errors());
    assert_eq!(invalid.stats.parsed.len(), 1);
    assert_eq!(session.last_successful(), Some(&valid));
    let repeated = project.update(&mut session, project.options())?;
    assert!(repeated.stats.parsed.is_empty());
    assert_eq!(repeated.program, invalid.program);
    project.write("leaf.ts", "export const value = 3;")?;
    let repaired = project.update(&mut session, project.options())?;
    assert!(!repaired.program.has_errors());
    assert_eq!(session.last_successful(), Some(&repaired.program));
    Ok(())
}

#[test]
fn deleted_recreated_and_detached_dependencies_do_not_leave_stale_entries() -> Result {
    let project = Project::new()?;
    project.write(
        "main.ts",
        "import { value } from './leaf'; export const result = value;",
    )?;
    let leaf = project.write("leaf.ts", "export const value = 1;")?;
    let mut session = AnalysisSession::new();
    project.update(&mut session, project.options())?;
    fs::remove_file(&leaf)?;
    let removed = project.update(&mut session, project.options())?;
    assert!(removed.program.has_errors());
    assert_eq!(removed.stats.evicted, vec![leaf.clone()]);
    assert_eq!(session.cached_modules(), 1);
    project.write("leaf.ts", "export const value = 1;")?;
    let restored = project.update(&mut session, project.options())?;
    assert!(!restored.program.has_errors());
    assert_eq!(restored.stats.parsed, vec![leaf.clone()]);
    project.write("main.ts", "export const result = 4;")?;
    let detached = project.update(&mut session, project.options())?;
    assert_eq!(detached.stats.evicted, vec![leaf]);
    assert_eq!(detached.program.modules.len(), 1);
    session.clear();
    assert_eq!(session.cached_modules(), 0);
    assert!(session.last_successful().is_none());
    assert_eq!(
        project
            .update(&mut session, project.options())?
            .stats
            .parsed
            .len(),
        1
    );
    Ok(())
}

#[test]
fn dependency_resolution_is_refreshed_even_when_all_importers_are_cached() -> Result {
    let project = Project::new()?;
    let main = project.write(
        "main.ts",
        "import { value } from './leaf.js'; export const result = value;",
    )?;
    let typed = project.write("leaf.ts", "export const value = 1;")?;
    let mut session = AnalysisSession::new();
    project.update(&mut session, project.options())?;
    let exact = project.write("leaf.js", "export const value = 2;")?;
    let update = project.update(&mut session, project.options())?;
    assert_eq!(update.stats.parsed, vec![exact.clone()]);
    assert_eq!(update.stats.evicted, vec![typed]);
    assert!(update.stats.affected.contains(&main));
    assert!(
        matches!(&update.program.module_graph.edges[0].target, Some(ResolvedModule::File { path }) if path == &exact)
    );
    Ok(())
}

#[test]
fn parser_and_resolver_option_changes_are_applied_without_false_cache_hits() -> Result {
    let project = Project::new()?;
    project.write(
        "main.ts",
        "import { value } from 'outside'; export const result: number = value;",
    )?;
    let mut session = AnalysisSession::new();
    let invalid = project.update(&mut session, project.options())?;
    assert!(invalid.program.has_errors());
    let mut options = project.options();
    options.resolve.externals.insert("outside".into());
    let valid = project.update(&mut session, options.clone())?;
    assert!(
        !valid.program.has_errors(),
        "{:?}",
        valid.program.diagnostics
    );
    assert!(valid.stats.parsed.is_empty());
    assert_eq!(valid.stats.affected.len(), 1);
    options.parse.language = Language::JavaScript;
    let wrong_language = project.update(&mut session, options)?;
    assert!(wrong_language.program.has_errors());
    assert_eq!(wrong_language.stats.parsed.len(), 1);
    assert_eq!(session.last_successful(), Some(&valid.program));
    Ok(())
}

#[test]
fn cycles_and_atomic_file_replacement_update_with_no_cache_leak() -> Result {
    let project = Project::new()?;
    project.write("main.ts", "export { a } from './a';")?;
    let a = project.write("a.ts", "import './b'; export const a = 1;")?;
    project.write("b.ts", "import './a'; export const b = 1;")?;
    let mut session = AnalysisSession::new();
    let initial = project.update(&mut session, project.options())?;
    assert_eq!(initial.program.module_graph.cycles.len(), 1);
    // 本测试目录内原子替换不依赖 mtime 精度或文件句柄身份。
    let replacement = project.write("replacement.tmp", "export const a = 2;")?;
    fs::remove_file(&a)?;
    fs::rename(&replacement, &a)?;
    let changed = project.update(&mut session, project.options())?;
    assert!(changed.program.module_graph.cycles.is_empty());
    assert_eq!(changed.stats.parsed, vec![a]);
    assert_eq!(session.cached_modules(), 2);
    Ok(())
}

#[test]
fn invalid_utf8_and_missing_root_preserve_last_successful() -> Result {
    let project = Project::new()?;
    let main = project.write("main.ts", "export const a = 1;")?;
    let mut session = AnalysisSession::new();
    let valid = project.update(&mut session, project.options())?.program;
    fs::write(&main, [0xff])?;
    let broken = project.update(&mut session, project.options())?;
    assert!(
        broken
            .program
            .diagnostics
            .iter()
            .any(|d| d.code == "KE3001")
    );
    assert_eq!(session.cached_modules(), 0);
    assert_eq!(session.last_successful(), Some(&valid));
    assert!(
        session
            .analyze_project(&project.0.join("missing"), project.options())
            .is_err()
    );
    assert_eq!(session.last_successful(), Some(&valid));
    Ok(())
}

#[test]
fn entry_changes_evict_previous_project_and_empty_entries_are_not_valid() -> Result {
    let project = Project::new()?;
    project.write("main.ts", "export const a = 1;")?;
    let other = project.write("other.ts", "export const b = 2;")?;
    let mut session = AnalysisSession::new();
    project.update(&mut session, project.options())?;
    let options = AnalyzeOptions {
        entry_points: vec![Path::new("other.ts").into()],
        ..Default::default()
    };
    let next = project.update(&mut session, options)?;
    assert_eq!(next.stats.parsed, vec![other]);
    assert_eq!(next.stats.evicted.len(), 1);
    let empty = project.update(&mut session, AnalyzeOptions::default())?;
    assert!(empty.program.has_errors());
    assert_eq!(session.cached_modules(), 0);
    assert_eq!(session.last_successful(), Some(&next.program));
    Ok(())
}
