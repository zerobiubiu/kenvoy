# Kenvoy / KEngine

Kenvoy 把 TypeScript/JSX 声明解释为具有页面、API、后台管理和 SQLite 持久化的完整应用。KEngine 是其中独立、通用的 Rust 源码解析库，不包含 Web 业务语义。

## 当前进度

当前默认 UI 使用 React + TypeScript + 原生 CSS，框架不再依赖 MUI/Emotion。select 支持静态联动和受控跨资源选项查询；系统组件可通过 override 声明替换显示层，自定义组件通过 SDK 继续使用原生资源、字段校验和 API。另支持 SQLite 持久化的多文件字段、附件下载及可显示上传图片/图片 URL 的 Image 组件。用法见[组件与选项契约](docs/contracts/component-bindings.md)、[文件与图片契约](docs/contracts/file-uploads.md)、[联动选项示例](examples/options-app/README.md)和[上传与图片示例](examples/uploads-app/README.md)。

首版基线已完成阶段 00—10，并在 Windows 上通过真实浏览器闭环：通用解析与增量分析、业务 DSL、真实 SQLite 和迁移、管理员认证、受控 HTTP CRUD、前后台、自定义组件、TypeScript/ESM 服务端函数、HMR，以及 create/dev/build/run/check/inspect/migrate。当时 201 项常规 Rust 测试、2 项显式跨工具测试和 90 项前端测试通过；历史命令与边界见[首版验收](docs/verification/0012-final-acceptance.md)，后续变更见[变更索引](docs/changes/README.md)。

产品仍由 crates/kengine（Library）和 crates/kenvoy（Binary）构成，依赖方向为 Kenvoy → KEngine。Workspace 另有专门的 [kenvoy-tests](crates/kenvoy-tests/) 测试成员和根目录 [xtask](xtask/) 工程编排成员，均不发布，产品不依赖它们。各产品 Crate 保留自己的独立 tests。详细状态与证据从 [文档索引](docs/README.md)进入；项目工作规则见 [AGENTS.md](AGENTS.md)。

内部代码已按 DDD 分层：领域模型与规则、应用用例与静态端口、基础设施、接口及宿主装配；前端同样分离纯规则、API 和展示。自有代码（含测试）每文件硬上限 500 行，目标 400 行内，由 `cargo xtask architecture/check` 检查。目录与依赖规则见[DDD 规范](docs/architecture/ddd-structure.md)，本次行为兼容验证见 [0016](docs/changes/0016-ddd-structure-and-module-split.md)。

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
cargo xtask check
```

KEngine 可单独 check/test，不要求数据库、HTTP 或浏览器。阶段 00 的 Rust HTTP/SQLite/JS/watcher 测试仍保留在各 Crate 的 tests 中，使用临时资源。架构与文档检查、构建和跨 Crate 测试统一入口见[工程与测试流程](docs/development/engineering-workflow.md)。

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

基础 DSL 运行不需要 Node/npm。自定义组件首次使用 `kenvoy dev --install-deps` 或 `kenvoy build --install-deps`，明确允许安装锁定的构建依赖；后续复用缓存，不每次重新安装。可运行示例见 [examples](examples/README.md)，扩展和安全限制见[JavaScript Runtime](docs/architecture/javascript-runtime.md)。

迁移先用 `kenvoy migrate status` 查看 diff；安全变更由启动/开发发布应用，破坏性变更须使用计划校验值明确确认。删除声明不会自动删除已有数据。`kenvoy check` 提供语法、模块和 DSL 检查，不能替代完整 TypeScript 类型检查器。

## 验证入口

`cargo xtask test` 先构建正式 CLI，再运行 Workspace 的标准 Rust 测试，包括 kenvoy-tests 中的跨模块分析、真实 HTTP、认证与持久化测试。Rust 夹具创建独立临时项目并管理 CLI 子进程；真实浏览器通过 `cargo xtask browser` 显式运行，覆盖 forms、fields、admin、extensions、options、watcher、production，Playwright 只负责浏览器操作和 DOM 断言。

`shutdown-observer.mjs` 配合独立终端 Ctrl+C，单独验证正常资源关闭与重启持久化。普通 Windows 子进程强制终止夹具不能替代这项退出验证。浏览器与前端依赖需事先准备；扩展/选项流程仅在各自临时项目内显式安装其声明并锁定的用户依赖。

```powershell
cargo xtask check
cargo xtask build
cargo xtask test
cargo xtask browser
```

原 tools 目录及独立前端探针已退出当前工程，历史验证报告保留原始命令和当时的边界；现行执行方式以[工程与测试流程](docs/development/engineering-workflow.md)为准。
