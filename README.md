# Kenvoy / KEngine

Kenvoy 把 TypeScript/JSX 声明解释为具有页面、API、后台管理和 SQLite 持久化的完整应用。KEngine 是其中独立、通用的 Rust 源码解析库，不包含 Web 业务语义。

## 当前进度

当前默认 UI 使用 React + TypeScript + 原生 CSS，框架不再依赖 MUI/Emotion。select 支持静态联动和受控跨资源选项查询；系统组件可通过 override 声明替换显示层，自定义组件通过 SDK 继续使用原生资源、字段校验和 API。用法见[组件与选项契约](docs/component-bindings.md)和[完整示例](examples/options-app/README.md)。

首版基线已完成阶段 00—10，并在 Windows 上通过真实浏览器闭环：通用解析与增量分析、业务 DSL、真实 SQLite 和迁移、管理员认证、受控 HTTP CRUD、前后台、自定义组件、TypeScript/ESM 服务端函数、HMR，以及 create/dev/build/run/check/inspect/migrate。当时 201 项常规 Rust 测试、2 项显式跨工具测试和 90 项前端测试通过；历史命令与边界见[首版验收](docs/verification/0012-final-acceptance.md)，后续变更见[变更索引](docs/changes/README.md)。

Workspace 只有两个自有包：crates/kengine（Library）和 crates/kenvoy（Binary），依赖方向为 Kenvoy → KEngine。详细状态与证据从 [文档索引](docs/README.md)进入；项目工作规则见 [AGENTS.md](AGENTS.md)。

## 从源码构建与验证

需要 Rust 1.98 或以上、原生 C 构建环境及 Node 24.22.0 或以上/npm。Kenvoy 构建脚本嵌入真实前端产物，首次 Cargo 构建前先准备前端；不会在 Cargo 构建脚本里自动下载依赖。仓库保留 Rust/npm 锁文件：

```powershell
npm ci --ignore-scripts --prefix crates/kenvoy/frontend
npm run check --prefix crates/kenvoy/frontend
npm test --prefix crates/kenvoy/frontend
npm run build --prefix crates/kenvoy/frontend
cargo build -p kenvoy --locked
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
node tools/verify-architecture.mjs
node tools/verify-docs.mjs
```

KEngine 可单独 check/test，不要求数据库、HTTP 或浏览器。阶段 00 的 HTTP/SQLite/JS/watcher 探针位于测试目录，使用临时资源。

## 创建与运行应用

在 Windows 上可以直接使用构建出的 CLI：

```powershell
$kenvoy = (Resolve-Path .\target\debug\kenvoy.exe).Path
& $kenvoy create customer-app
Set-Location customer-app
& $kenvoy admin init --username admin
& $kenvoy dev
```

`admin init` 隐藏输入密码，不提供默认密码。访问 `/customers` 登记客户，登录 `/admin/customers` 查询、搜索、分页、排序、编辑和删除。匿名仅有声明允许的表单提交权限，不能读取列表。数据位于 `data/app.sqlite`，开发与生产共用此路径。

停止开发服务后执行 `kenvoy run` 即可启动生产应用，无需先 build。源码或依赖锁变化会触发构建；无源码时可部署 CLI、kenvoy.toml、dist 和独立数据目录。静态服务只暴露 client 资产。`kenvoy build` 不修改业务数据库。

基础 DSL 运行不需要 Node/npm。自定义组件首次使用 `kenvoy dev --install-deps` 或 `kenvoy build --install-deps`，明确允许安装锁定的构建依赖；后续复用缓存，不每次重新安装。可运行示例见 [examples](examples/README.md)，扩展和安全限制见[JavaScript Runtime](docs/javascript-runtime.md)。

迁移先用 `kenvoy migrate status` 查看 diff；安全变更由启动/开发发布应用，破坏性变更须使用计划校验值明确确认。删除声明不会自动删除已有数据。`kenvoy check` 提供语法、模块和 DSL 检查，不能替代完整 TypeScript 类型检查器。

## 验证入口

`crates/kenvoy/frontend/scripts/` 提供 forms、fields、admin、extensions、watcher、production E2E，均使用真实 CLI/API/SQLite 与 Chromium；夹具创建独立临时项目。`shutdown-observer.mjs` 配合独立终端 Ctrl+C，单独验证正常资源关闭与重启持久化。普通 Windows 子进程强制终止夹具不能替代这项退出验证。

## 浏览器技术探针

tools/probes/frontend 是独立兼容性探针，使用锁定 npm 依赖，不是正式用户项目或 Kenvoy Runtime。复现方式和限制见 [探针说明](tools/probes/frontend/README.md)，全程只监听 loopback。

显式跨工具测试需要先构建该目录并准备 Playwright，再运行：

```powershell
cargo test -p kenvoy --test p03_http_sqlite axum_serves_built_react_and_sqlite_to_real_chromium --locked -- --ignored --nocapture
```

普通 cargo test 会明确忽略该测试，避免要求仅使用 KEngine 的开发者安装前端工具。显式执行结果见 [Axum/React/Chromium 报告](docs/verification/0002-axum-react-browser.md)。
