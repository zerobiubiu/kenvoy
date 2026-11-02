//! 显式仓库、只读 Git 查询与仅创建引用的标签事务。

use super::{
    manifest::is_release_version,
    plan::{ExistingTag, NewTag},
};
use crate::Result;
use std::{
    fmt::Write as _,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

pub(super) struct Repository {
    pub(super) path: PathBuf,
}

impl Repository {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let requested = fs::canonicalize(path)
            .map_err(|error| format!("无法打开仓库目录 {}：{error}", path.display()))?;
        let mut repository = Self {
            path: path.to_path_buf(),
        };
        let root = repository.read(&["rev-parse", "--show-toplevel"])?;
        if fs::canonicalize(&root)? != requested {
            return Err(format!(
                "必须明确指定 Git 工作区根目录 {root}，不能通过子目录隐式选择父仓库"
            )
            .into());
        }
        repository.path = PathBuf::from(root);
        if repository.read(&["rev-parse", "--is-shallow-repository"])? != "false" {
            return Err("版本标签需要完整 dev 历史；浅仓库不能预览或创建标签".into());
        }
        Ok(repository)
    }

    pub(super) fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .args(["--no-optional-locks", "--no-replace-objects", "-C"])
            .arg(&self.path)
            .env("GIT_NO_LAZY_FETCH", "1")
            .stdin(Stdio::null());
        // Git 钩子会导出这些变量；不能让它们覆盖 --repo 指定的隔离仓库。
        for variable in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_NAMESPACE",
            "GIT_SHALLOW_FILE",
        ] {
            command.env_remove(variable);
        }
        super::super::process::hide_window(&mut command);
        command
    }

    pub(super) fn read(&self, arguments: &[&str]) -> Result<String> {
        let output = self
            .command()
            .args(arguments)
            .output()
            .map_err(|error| format!("无法启动 Git：{error}"))?;
        decode(arguments, output)
    }

    fn input(&self, arguments: &[&str], input: &[u8]) -> Result<String> {
        let mut child = self
            .command()
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法启动 Git：{error}"))?;
        let written = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("Git 子进程缺少标准输入"))
            .and_then(|mut pipe| pipe.write_all(input));
        let output = child.wait_with_output()?;
        let text = decode(arguments, output)?;
        written.map_err(|error| format!("无法向 Git 写入标签事务：{error}"))?;
        Ok(text)
    }

    pub(super) fn require_dev(&self) -> Result<()> {
        let output = self
            .command()
            .args(["symbolic-ref", "--quiet", "HEAD"])
            .output()?;
        if output.status.code() == Some(1) {
            return Err("--apply 仅允许当前 dev 分支；当前为分离 HEAD".into());
        }
        let branch = decode(&["symbolic-ref", "--quiet", "HEAD"], output)?;
        if branch != "refs/heads/dev" {
            return Err(format!("--apply 仅允许当前 dev 分支；当前为 {branch}").into());
        }
        Ok(())
    }

    pub(super) fn version_tags(&self) -> Result<Vec<ExistingTag>> {
        let output = self.read(&[
            "for-each-ref",
            "--format=%(refname:strip=2)%00%(objecttype)%00%(objectname)%00%(*objecttype)%00%(*objectname)",
            "refs/tags/",
        ])?;
        let mut tags = Vec::new();
        for line in output.lines() {
            let mut fields = line.split('\0');
            let (Some(name), Some(kind), Some(object), Some(target_kind), Some(target)) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            ) else {
                return Err("Git 标签列表格式无效".into());
            };
            if !name.strip_prefix('v').is_some_and(is_release_version) {
                continue;
            }
            if kind != "tag" {
                return Err(format!(
                    "版本标签 {name} 是轻量标签或非 annotated 标签；要求冲突，不会自动转换或覆盖"
                )
                .into());
            }
            let commit = if target_kind == "commit" {
                target.to_owned()
            } else {
                self.read(&[
                    "rev-parse",
                    "--verify",
                    &format!("refs/tags/{name}^{{commit}}"),
                ])?
            };
            tags.push(ExistingTag {
                name: name.to_owned(),
                object: object.to_owned(),
                commit,
            });
        }
        Ok(tags)
    }

    pub(super) fn create_tags(
        &self,
        head: &str,
        existing: &[ExistingTag],
        missing: &[NewTag],
    ) -> Result<()> {
        if missing.is_empty() {
            return Ok(());
        }
        self.require_dev()?;
        let identity = self.read(&["var", "GIT_COMMITTER_IDENT"])?;
        let mut transaction = format!("start\nverify refs/heads/dev {head}\n");
        for tag in existing {
            writeln!(transaction, "verify refs/tags/{} {}", tag.name, tag.object)?;
        }
        for tag in missing {
            let contents = format!(
                "object {}\ntype commit\ntag {}\ntagger {identity}\n\n{}\n",
                tag.commit, tag.name, tag.message
            );
            let object = self.input(&["mktag"], contents.as_bytes())?;
            writeln!(transaction, "create refs/tags/{} {object}", tag.name)?;
        }
        transaction.push_str("prepare\ncommit\n");
        self.require_dev()?;
        self.input(&["update-ref", "--stdin"], transaction.as_bytes())
            .map_err(|error| {
                format!("无法确认标签创建事务完成，请重新预览；不会强制覆盖或移动既有标签：{error}")
            })?;
        Ok(())
    }
}

fn decode(arguments: &[&str], output: Output) -> Result<String> {
    if !output.status.success() {
        return Err(format!(
            "Git {} 失败（{}）：{}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let mut text =
        String::from_utf8(output.stdout).map_err(|error| format!("Git 输出不是 UTF-8：{error}"))?;
    text.truncate(text.trim_end_matches(['\r', '\n']).len());
    Ok(text)
}
