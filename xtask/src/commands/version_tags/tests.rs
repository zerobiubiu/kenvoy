//! 使用真实独立临时 Git 仓库，不接触本仓库的引用、配置或工作区。

use super::{
    execute,
    git::Repository,
    manifest::{is_release_version, parse_manifest},
};
use crate::Result;
use std::{
    ffi::OsString,
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    repository: Repository,
}

impl Fixture {
    fn directory() -> Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kenvoy-version-tags-{}-{timestamp}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self {
            repository: Repository { path },
        })
    }

    fn new() -> Result<Self> {
        let fixture = Self::directory()?;
        fixture.git(&["init", "--initial-branch=dev"])?;
        for (key, value) in [
            ("user.name", "版本标签回归"),
            ("user.email", "version-tags@example.invalid"),
            ("commit.gpgSign", "false"),
            ("tag.gpgSign", "false"),
            ("core.hooksPath", ".disabled-hooks"),
        ] {
            fixture.git(&["config", "--local", key, value])?;
        }
        Ok(fixture)
    }

    fn path(&self) -> &Path {
        &self.repository.path
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        self.repository.read(arguments)
    }

    fn write_manifest(&self, version: &str) -> Result<()> {
        let directory = self.path().join("crates/kenvoy");
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname = \"kenvoy\"\nversion = \"{version}\"\n"),
        )?;
        Ok(())
    }

    fn commit_version(&self, version: &str) -> Result<String> {
        self.write_manifest(version)?;
        self.commit("更新已提交版本")
    }

    fn commit_source(&self, contents: &str) -> Result<String> {
        fs::write(self.path().join("source.txt"), contents)?;
        self.commit("只修改源码")
    }

    fn commit(&self, message: &str) -> Result<String> {
        self.git(&["add", "--all"])?;
        self.git(&["commit", "--message", message])?;
        self.git(&["rev-parse", "HEAD"])
    }

    fn run(&self, apply: bool) -> Result<()> {
        let mut arguments = vec![OsString::from("--repo"), self.path().as_os_str().to_owned()];
        if apply {
            arguments.push("--apply".into());
        }
        execute(self.path(), &arguments)
    }

    fn tags(&self) -> Result<String> {
        self.git(&[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/tags/",
        ])
    }

    fn annotated_tag(&self, name: &str, commit: &str) -> Result<()> {
        self.git(&[
            "tag",
            "--annotate",
            "--no-sign",
            "--message",
            "保留既有标签",
            name,
            commit,
        ])?;
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(self.path()) {
            eprintln!(
                "无法清理版本标签测试仓库 {}：{error}",
                self.path().display()
            );
        }
    }
}

#[test]
fn preview_and_apply_use_committed_versions_and_are_idempotent() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = fixture.commit_version("0.1.0")?;
    fixture.commit_source("同版本源码提交")?;
    fixture.run(false)?;
    assert!(fixture.tags()?.is_empty());

    fixture.write_manifest("9.9.9")?;
    fixture.git(&["add", "crates/kenvoy/Cargo.toml"])?;
    let worktree = fixture.git(&["status", "--porcelain"])?;
    fixture.run(true)?;
    assert_eq!(fixture.git(&["rev-parse", "v0.0.0^{commit}"])?, root);
    assert_eq!(fixture.git(&["cat-file", "-t", "v0.0.0"])?, "tag");
    assert_eq!(
        fixture.git(&["for-each-ref", "--format=%(contents)", "refs/tags/v0.0.0"])?,
        "0.0.0 init"
    );
    assert_eq!(fixture.git(&["tag", "--list"])?, "v0.0.0");
    assert_eq!(fixture.git(&["status", "--porcelain"])?, worktree);
    let tags = fixture.tags()?;
    fixture.run(true)?;
    assert_eq!(fixture.tags()?, tags);
    Ok(())
}

#[test]
fn new_versions_use_first_declaration_and_same_version_commits_do_not_move_tags() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.commit_version("0.1.0")?;
    let first = fixture.commit_version("0.2.0")?;
    fixture.run(true)?;
    let tags = fixture.tags()?;
    fixture.commit_source("已打标签后的同版本修改")?;
    fixture.run(true)?;
    assert_eq!(fixture.tags()?, tags);
    assert_eq!(fixture.git(&["rev-parse", "v0.2.0^{commit}"])?, first);

    let patch = fixture.commit_version("0.2.1")?;
    fixture.run(false)?;
    assert_eq!(fixture.tags()?, tags);
    fixture.run(true)?;
    assert_eq!(fixture.git(&["rev-parse", "v0.2.1^{commit}"])?, patch);
    assert_eq!(fixture.git(&["rev-parse", "v0.2.0^{commit}"])?, first);
    assert_eq!(fixture.git(&["cat-file", "-t", "v0.2.1"])?, "tag");
    Ok(())
}

#[test]
fn existing_tag_at_later_same_version_commit_is_preserved() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.commit_version("0.1.0")?;
    fixture.commit_version("0.2.0")?;
    let later = fixture.commit_source("既有发布提交")?;
    fixture.annotated_tag("v0.2.0", &later)?;
    let object = fixture.git(&["rev-parse", "refs/tags/v0.2.0"])?;
    fixture.run(true)?;
    assert_eq!(fixture.git(&["rev-parse", "refs/tags/v0.2.0"])?, object);
    assert_eq!(fixture.git(&["rev-parse", "v0.2.0^{commit}"])?, later);
    let tags = fixture.tags()?;
    fixture.run(true)?;
    assert_eq!(fixture.tags()?, tags);
    Ok(())
}

#[test]
fn conflicting_tags_fail_without_creating_other_tags() -> Result<()> {
    for (name, annotated) in [("v0.3.0", true), ("v0.2.0", false), ("v0.0.0", true)] {
        let fixture = Fixture::new()?;
        fixture.commit_version("0.1.0")?;
        let current = fixture.commit_version("0.2.0")?;
        if annotated {
            fixture.annotated_tag(name, &current)?;
        } else {
            fixture.git(&["tag", name, &current])?;
        }
        let tags = fixture.tags()?;
        assert!(fixture.run(true).is_err());
        assert_eq!(fixture.tags()?, tags);
    }
    Ok(())
}

#[test]
fn apply_rejects_main_and_detached_head_while_preview_remains_read_only() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.commit_version("0.1.0")?;
    fixture.commit_version("0.2.0")?;
    fixture.git(&["switch", "--create", "main"])?;
    fixture.run(false)?;
    assert!(fixture.run(true).is_err());
    fixture.git(&["switch", "--detach", "refs/heads/dev"])?;
    assert!(fixture.run(true).is_err());
    assert!(fixture.tags()?.is_empty());
    Ok(())
}

#[test]
fn foreign_history_tags_and_shallow_clones_are_rejected() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.commit_version("0.1.0")?;
    fixture.commit_version("0.2.0")?;
    fixture.git(&["switch", "--orphan", "unrelated"])?;
    let foreign = fixture.commit_version("0.2.0")?;
    fixture.annotated_tag("v0.2.0", &foreign)?;
    fixture.git(&["switch", "dev"])?;
    let tags = fixture.tags()?;
    assert!(fixture.run(true).is_err());
    assert_eq!(fixture.tags()?, tags);

    let shallow = Fixture::directory()?;
    fixture.git(&[
        "clone",
        "--no-local",
        "--depth",
        "1",
        "--branch",
        "dev",
        fixture.path().to_str().ok_or("测试仓库路径不是 UTF-8")?,
        shallow.path().to_str().ok_or("测试仓库路径不是 UTF-8")?,
    ])?;
    assert_eq!(
        shallow.git(&["rev-parse", "--is-shallow-repository"])?,
        "true"
    );
    let shallow_tags = shallow.tags()?;
    assert!(shallow.run(true).is_err());
    assert_eq!(shallow.tags()?, shallow_tags);
    Ok(())
}

#[test]
fn merge_uses_original_version_introduction_without_tagging_root_baseline() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.commit_version("0.1.0")?;
    fixture.git(&["switch", "--create", "topic"])?;
    let introduced = fixture.commit_version("0.2.0")?;
    fixture.git(&["switch", "dev"])?;
    fixture.commit_source("dev 继续沿用根的旧版本")?;
    fixture.git(&["merge", "--no-ff", "--message", "合并版本变更", "topic"])?;
    fixture.run(true)?;
    assert_eq!(fixture.git(&["rev-parse", "v0.2.0^{commit}"])?, introduced);
    assert_eq!(fixture.git(&["tag", "--list"])?, "v0.0.0\nv0.2.0");
    Ok(())
}

#[test]
fn versions_are_parsed_as_toml_and_reject_non_release_semver() -> Result<()> {
    for version in ["0.0.0", "1.2.3", "123456789012345678901234567890.2.3"] {
        assert!(is_release_version(version));
    }
    for version in [
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-rc.1",
        "1.2.3+build",
        "１.2.3",
        " 1.2.3",
    ] {
        assert!(!is_release_version(version));
    }
    assert_eq!(
        parse_manifest(b"[package]\nname = 'kenvoy'\nversion = '1.2.3' # committed\n")?.as_ref(),
        "1.2.3"
    );
    assert!(parse_manifest(b"[package]\nname = 'another'\nversion = '1.2.3'\n").is_err());
    assert!(parse_manifest(b"[package]\nname = 'kenvoy'\nversion.workspace = true\n").is_err());
    Ok(())
}
