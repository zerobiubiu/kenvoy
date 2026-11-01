use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde_json::Value;
use tokio::{io::AsyncReadExt, process::Command};

use super::{
    ClientBuildError, Result,
    error::{invalid, io},
};
use crate::infrastructure::project::Project;

pub(super) fn npm_cli(node: &Path) -> Result<PathBuf> {
    let mut directories = vec![
        node.parent()
            .ok_or_else(|| invalid("Node 可执行文件缺少父目录"))?
            .to_path_buf(),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&path));
    }
    for directory in directories {
        for suffix in [
            "node_modules/npm/bin/npm-cli.js",
            "bin/node_modules/npm/bin/npm-cli.js",
            "../lib/node_modules/npm/bin/npm-cli.js",
        ] {
            let candidate = directory.join(suffix);
            if candidate.is_file() {
                return candidate
                    .canonicalize()
                    .map_err(|error| io(candidate, error));
            }
        }
        let npm = directory.join("npm");
        if let Ok(resolved) = npm.canonicalize()
            && resolved.file_name() == Some(OsStr::new("npm-cli.js"))
        {
            return Ok(resolved);
        }
    }
    Err(invalid(
        "未找到 npm-cli.js；自定义客户端需要 Node.js 配套 npm",
    ))
}

pub(super) async fn npm_command(
    node: &Path,
    npm: &Path,
    directory: &Path,
    arguments: &[&str],
    windows_null: bool,
) -> Result<()> {
    let mut args = vec![npm.as_os_str().to_owned()];
    args.extend(arguments.iter().map(|argument| {
        OsString::from(if windows_null && *argument == "/dev/null" {
            "NUL"
        } else {
            argument
        })
    }));
    run(node, &args, directory, "锁定 npm 依赖准备", 180)
        .await
        .map(|_| ())
}

pub(super) async fn install_project_dependencies(project: &Project, node: &Path) -> Result<()> {
    let path = project.root.join("package.json");
    if !path.is_file() {
        return Ok(());
    }
    let original = fs::read(&path).map_err(|error| io(&path, error))?;
    let manifest: Value = serde_json::from_slice(&original)?;
    for key in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        if let Some(entries) = manifest.get(key).and_then(Value::as_object) {
            for (name, version) in entries {
                let version = version
                    .as_str()
                    .ok_or_else(|| invalid("npm 依赖版本必须是字符串"))?;
                if version.is_empty()
                    || version.contains([':', '/', '\\', '#'])
                    || version.chars().any(char::is_control)
                {
                    return Err(invalid(format!(
                        "依赖 {name} 仅允许 npm registry 版本；不能自动安装 Git、文件或工作区来源"
                    )));
                }
            }
        }
    }
    let npm = npm_cli(node)?;
    if !project.root.join("package-lock.json").is_file() {
        npm_command(
            node,
            &npm,
            &project.root,
            &[
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ],
            false,
        )
        .await?;
    }
    npm_command(
        node,
        &npm,
        &project.root,
        &[
            "ci",
            "--ignore-scripts",
            "--include=dev",
            "--no-audit",
            "--no-fund",
        ],
        false,
    )
    .await?;
    if fs::read(&path).map_err(|error| io(&path, error))? != original {
        return Err(invalid("npm 改变了用户 package.json；请检查改动后重新构建"));
    }
    Ok(())
}

pub(super) async fn drain<R: tokio::io::AsyncRead + Unpin>(
    mut input: R,
) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = input.read(&mut buffer).await?;
        if count == 0 {
            return Ok(result);
        }
        let keep = count.min((256 * 1024_usize).saturating_sub(result.len()));
        result.extend_from_slice(&buffer[..keep]);
    }
}
pub(super) async fn run(
    executable: impl AsRef<OsStr>,
    args: &[OsString],
    directory: &Path,
    operation: &str,
    seconds: u64,
) -> Result<String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(directory)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env("NODE_ENV", "production")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("子进程缺少 stdout 管道"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("子进程缺少 stderr 管道"))?;
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
        tokio::join!(child.wait(), drain(stdout), drain(stderr))
    })
    .await;
    let (status, out, err) = match result {
        Ok(value) => value,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(ClientBuildError::Timeout {
                operation: operation.into(),
                seconds,
            });
        }
    };
    let status = status.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?;
    let out = String::from_utf8_lossy(&out.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?)
    .into_owned();
    let err = String::from_utf8_lossy(&err.map_err(|source| ClientBuildError::Spawn {
        operation: operation.into(),
        source,
    })?)
    .into_owned();
    if !status.success() {
        return Err(ClientBuildError::Process {
            operation: operation.into(),
            code: status.code(),
            output: format!("{out}\n{err}"),
        });
    }
    if !err.trim().is_empty() {
        tracing::warn!(target:"kenvoy::build", "{}",err.trim());
    }
    Ok(out)
}
