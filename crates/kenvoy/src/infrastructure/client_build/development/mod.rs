//! Vite 只见到最后发布的客户端镜像；原始源码变化由 Rust 候选生命周期控制。
mod mirror;
mod protocol;

use std::{
    collections::BTreeMap,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kengine::ProgramIR;
use serde_json::{Value, json};
use tokio::{
    io::BufReader,
    process::{Child, ChildStdin, ChildStdout, Command},
    task::JoinHandle,
};

use super::{
    ClientBuildError, ClientBuildOptions, Result, build,
    cache::{directory as ensure_directory, immutable},
    command::drain,
    error::{invalid, io},
};
use crate::{domain::model::ApplicationIR, infrastructure::project::Project};
use mirror::{hashes, snapshot, write_mirror};

pub struct DevClient {
    root: PathBuf,
    directory: PathBuf,
    mirror: PathBuf,
    endpoint: SocketAddr,
    secret: String,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    log: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    sequence: u64,
    hashes: BTreeMap<String, String>,
    closed: bool,
}

pub struct DevUpdate {
    root: PathBuf,
    request: Value,
    files: BTreeMap<String, Vec<u8>>,
}

impl DevClient {
    pub async fn start(
        project: &Project,
        program: &ProgramIR,
        app: &ApplicationIR,
        options: ClientBuildOptions,
        public_origin: &str,
    ) -> Result<Self> {
        let origin = url::Url::parse(public_origin).map_err(|_| invalid("开发入口 Origin 无效"))?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.origin().ascii_serialization() != public_origin
        {
            return Err(invalid("开发入口必须是完整 HTTP(S) Origin"));
        }
        let root = project
            .root
            .canonicalize()
            .map_err(|error| io(&project.root, error))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("系统时间无效"))?
            .as_nanos();
        let directory = ensure_directory(
            &root,
            &root
                .join(".kenvoy/client-dev")
                .join(format!("{}-{stamp}", std::process::id())),
        )?;
        let mirror = ensure_directory(&root, &directory.join("mirror"))?;
        let cache = ensure_directory(&root, &directory.join("vite-cache"))?;
        let output = ensure_directory(&root, &directory.join("initial-build"))?;
        let before = crate::infrastructure::artifacts::input_fingerprint(project)?;
        let prepared = build(project, program, app, &output, options).await?;
        let seed = prepared
            .seed
            .ok_or_else(|| invalid("基础 DSL 无需启动 Vite 开发服务"))?;
        let update = snapshot(project, program, app, &seed.request, &mirror)?;
        if crate::infrastructure::artifacts::input_fingerprint(project)? != before {
            return Err(invalid("准备开发服务期间源码或资产改变，请重新构建候选"));
        }
        write_mirror(&root, &mirror, &BTreeMap::new(), &update.files)?;
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| invalid("无法生成开发服务内部凭据"))?;
        let secret = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let request_path = directory.join("start.json");
        immutable(
            &request_path,
            &serde_json::to_vec(
                &json!({"secret":secret,"public_origin":public_origin,"request":update.request,"files":update.files.keys().collect::<Vec<_>>(),"cache":cache}),
            )?,
        )?;
        let framework = PathBuf::from(
            seed.request["framework_root"]
                .as_str()
                .ok_or_else(|| invalid("缺少框架源码路径"))?,
        );
        let mut command = Command::new(&seed.node);
        command
            .arg(framework.join("client-dev.mjs"))
            .arg(&request_path)
            .current_dir(&directory)
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .env_remove("__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS")
            .env("NODE_ENV", "development")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn().map_err(|source| ClientBuildError::Spawn {
            operation: "Vite 开发服务".into(),
            source,
        })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| invalid("开发服务缺少控制管道"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| invalid("开发服务缺少响应管道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| invalid("开发服务缺少日志管道"))?;
        let log = tokio::spawn(drain(stderr));
        let mut client = Self {
            root,
            directory,
            mirror,
            endpoint: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            secret,
            child,
            input,
            output: BufReader::new(stdout),
            log: Some(log),
            sequence: 0,
            hashes: hashes(&update.files),
            closed: false,
        };
        let ready = client.reply(0).await;
        // 凭据只保留内存及已经打开的管道，不长期留在缓存 JSON 中。
        fs::remove_file(&request_path).map_err(|error| io(&request_path, error))?;
        match ready {
            Ok(reply) => {
                let port = reply
                    .get("port")
                    .and_then(Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok())
                    .filter(|port| *port != 0)
                    .ok_or_else(|| invalid("开发服务未返回有效监听端口"))?;
                client.endpoint.set_port(port);
                Ok(client)
            }
            Err(error) => {
                let _ = client.stop_process().await;
                Err(error)
            }
        }
    }

    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    pub fn proxy_secret(&self) -> &str {
        &self.secret
    }

    pub async fn prepare_update(
        &self,
        project: &Project,
        program: &ProgramIR,
        app: &ApplicationIR,
    ) -> Result<DevUpdate> {
        if self.closed || project.root != self.root {
            return Err(invalid("开发服务已经关闭或项目根发生变化"));
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("系统时间无效"))?
            .as_nanos();
        let output = ensure_directory(
            &self.root,
            &self.directory.join(format!("candidate-{stamp}")),
        )?;
        let before = crate::infrastructure::artifacts::input_fingerprint(project)?;
        let candidate = build(
            project,
            program,
            app,
            &output,
            ClientBuildOptions::default(),
        )
        .await?;
        let seed = candidate
            .seed
            .ok_or_else(|| invalid("客户端引用已全部移除；应关闭 Vite 并使用内置资产"))?;
        let update = snapshot(project, program, app, &seed.request, &self.mirror)?;
        if crate::infrastructure::artifacts::input_fingerprint(project)? != before {
            return Err(invalid("准备开发镜像期间源码或资产改变，候选未发布"));
        }
        Ok(update)
    }

    pub async fn commit_update(&mut self, update: DevUpdate, structural: bool) -> Result<()> {
        if self.closed || update.root != self.root {
            return Err(invalid("不能发布其他项目或已关闭的开发候选"));
        }
        let next = hashes(&update.files);
        let changed: Vec<_> = next
            .iter()
            .filter(|(name, hash)| self.hashes.get(*name) != Some(*hash))
            .map(|(name, _)| name.clone())
            .collect();
        let removed = self.hashes.keys().any(|name| !next.contains_key(name));
        write_mirror(&self.root, &self.mirror, &self.hashes, &update.files)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| invalid("开发控制序号已耗尽"))?;
        let request_path = self.directory.join(format!("apply-{}.json", self.sequence));
        immutable(
            &request_path,
            &serde_json::to_vec(
                &json!({"request":update.request,"files":update.files.keys().collect::<Vec<_>>(),"changed":changed}),
            )?,
        )?;
        let result=self.command(json!({"id":self.sequence,"type":"apply","path":request_path,"structural":structural||removed})).await;
        fs::remove_file(&request_path).map_err(|error| io(&request_path, error))?;
        result?;
        self.hashes = next;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if self.closed {
            self.remove_workdir();
            return Ok(());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| invalid("开发控制序号已耗尽"))?;
        // 正常关闭优先让 Vite 释放端口与子进程；控制管道失效时不无限等待。
        let result = match tokio::time::timeout(
            Duration::from_secs(3),
            self.command(json!({"id":self.sequence,"type":"shutdown"})),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(invalid("开发服务关闭控制超时")),
        };
        let stopped = self.stop_process().await;
        self.remove_workdir();
        result?;
        stopped
    }

    /// 只删除本次开发实例独占创建的镜像、缓存与初始构建目录。
    fn remove_workdir(&self) {
        let parent = self.directory.parent().map(|parent| parent.to_path_buf());
        match fs::remove_dir_all(&self.directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(target:"kenvoy::build",%error,path=%self.directory.display(),"开发临时目录清理失败");
            }
        }
        if let Some(parent) = parent {
            let _ = fs::remove_dir(parent);
        }
    }
}

impl Drop for DevClient {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.start_kill();
        }
        if let Some(log) = self.log.take() {
            log.abort();
        }
    }
}
