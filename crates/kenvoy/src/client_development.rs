//! Vite 只见到最后发布的客户端镜像；原始源码变化由 Rust 候选生命周期控制。
use super::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    task::JoinHandle,
};

const MAX_MIRROR_FILES: usize = 8192;
const MAX_MIRROR_FILE: usize = 32 * 1024 * 1024;
const MAX_MIRROR_TOTAL: usize = 256 * 1024 * 1024;

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
        let directory = directory(
            &root,
            &root
                .join(".kenvoy/client-dev")
                .join(format!("{}-{stamp}", std::process::id())),
        )?;
        let mirror = super::directory(&root, &directory.join("mirror"))?;
        let cache = super::directory(&root, &directory.join("vite-cache"))?;
        let output = super::directory(&root, &directory.join("initial-build"))?;
        let before = crate::artifact::input_fingerprint(project)?;
        let prepared = build(project, program, app, &output, options).await?;
        let seed = prepared
            .seed
            .ok_or_else(|| invalid("基础 DSL 无需启动 Vite 开发服务"))?;
        let update = snapshot(project, program, app, &seed.request, &mirror)?;
        if crate::artifact::input_fingerprint(project)? != before {
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
        let output = directory(
            &self.root,
            &self.directory.join(format!("candidate-{stamp}")),
        )?;
        let before = crate::artifact::input_fingerprint(project)?;
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
        if crate::artifact::input_fingerprint(project)? != before {
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
            return Ok(());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| invalid("开发控制序号已耗尽"))?;
        let result = self
            .command(json!({"id":self.sequence,"type":"shutdown"}))
            .await;
        self.closed = true;
        let stopped = self.stop_process().await;
        result?;
        stopped
    }

    async fn command(&mut self, value: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        if bytes.len() > 4096 {
            return Err(invalid("开发控制指令超过限制"));
        }
        tokio::time::timeout(Duration::from_secs(20), self.input.write_all(&bytes))
            .await
            .map_err(|_| ClientBuildError::Timeout {
                operation: "开发控制写入".into(),
                seconds: 20,
            })?
            .map_err(|source| ClientBuildError::Spawn {
                operation: "开发控制写入".into(),
                source,
            })?;
        self.reply(self.sequence).await.map(|_| ())
    }

    async fn reply(&mut self, id: u64) -> Result<Value> {
        let mut bytes = Vec::new();
        let count = tokio::time::timeout(
            Duration::from_secs(30),
            (&mut self.output).take(4097).read_until(b'\n', &mut bytes),
        )
        .await
        .map_err(|_| ClientBuildError::Timeout {
            operation: "开发控制响应".into(),
            seconds: 30,
        })?
        .map_err(|source| ClientBuildError::Spawn {
            operation: "开发控制响应".into(),
            source,
        })?;
        if count == 0 || count > 4096 || bytes.last() != Some(&b'\n') {
            return Err(invalid("开发服务关闭或返回无效控制响应"));
        }
        let response: Value = serde_json::from_slice(&bytes)?;
        if response["id"].as_u64() != Some(id) {
            return Err(invalid("开发控制响应序号不匹配"));
        }
        if response["ok"] != true {
            return Err(invalid(
                response["error"].as_str().unwrap_or("开发服务拒绝候选更新"),
            ));
        }
        Ok(response)
    }

    async fn stop_process(&mut self) -> Result<()> {
        match tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(source)) => {
                return Err(ClientBuildError::Spawn {
                    operation: "等待 Vite 服务退出".into(),
                    source,
                });
            }
            Err(_) => {
                let _ = self.child.kill().await;
                self.child
                    .wait()
                    .await
                    .map_err(|source| ClientBuildError::Spawn {
                        operation: "回收 Vite 服务".into(),
                        source,
                    })?;
            }
        }
        if let Some(log) = self.log.take() {
            let bytes = log
                .await
                .map_err(|error| invalid(format!("开发日志任务失败：{error}")))?
                .map_err(|source| ClientBuildError::Spawn {
                    operation: "读取 Vite 日志".into(),
                    source,
                })?;
            let text = String::from_utf8_lossy(&bytes);
            if !text.trim().is_empty() {
                tracing::debug!(target:"kenvoy::build","{}",text.trim());
            }
        }
        self.closed = true;
        Ok(())
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

fn hashes(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect()
}

fn snapshot(
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

fn write_mirror(
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mirror_budget_is_checked_before_allocation_and_stream_reads_are_bounded() -> Result<()> {
        assert!(remaining_capacity(MAX_MIRROR_FILES, 0).is_err());
        assert!(remaining_capacity(0, MAX_MIRROR_TOTAL + 1).is_err());
        assert_eq!(remaining_capacity(0, MAX_MIRROR_TOTAL - 3)?, 3);
        assert_eq!(remaining_capacity(0, 0)?, MAX_MIRROR_FILE);
        let mut input = std::io::Cursor::new([7_u8; 64]);
        assert!(read_bounded(&mut input, 8).is_err());
        assert_eq!(input.position(), 9);
        assert_eq!(
            read_bounded(std::io::Cursor::new([7_u8; 8]), 8)
                .map_err(|error| io("测试流", error))?,
            vec![7; 8]
        );
        Ok(())
    }
}
