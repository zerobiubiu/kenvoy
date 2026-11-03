# Kenvoy — 汇批呈览

**面向 AI 时代的语义化低代码平台 · 写语义，出全端。**

[![Packaging status](https://repology.org/badge/vertical-allrepos/kenvoy.svg)](https://repology.org/project/kenvoy/versions)

Kenvoy 把 TypeScript/JSX 声明解释为具有页面、API、后台管理和 SQLite 持久化的完整应用。KEngine 是其中独立、通用的 Rust 源码解析库，不包含 Web 业务语义。

## 定位与命名

**Kenvoy** 是一套面向 AI 时代的语义化低代码平台：开发者或 AI 用语义声明描述业务，Kenvoy 解析后生成可运行的前后端页面，并关联数据模型、权限与持久化。它不是拖拽式低代码——拖拽产出坐标与配置、丢失业务语义、AI 难以可靠操作；Kenvoy 让书写方只表达极简语义，由引擎承担解析、接线与实现细节。

当前 DSL 为 **JSX/TSX 声明 + `kenvoy.toml`**（见[已实现声明语法](docs/contracts/dsl-specification.md)、[项目配置](docs/contracts/project-configuration.md)）。0.1 时期曾规划以 XML 描述业务，XML 不属于当前能力；以本仓库实现与验证记录为准。

**命名来历**：Kenvoy 读作 `/ˈkɛn.vɔɪ/`，由 **Ken**（古英语：知悉、看见、视野）与 **voy**（法语/英语 Voyage：旅程、流转）构成，意为「知悉一切，流转万物」。

**中文名「汇批呈览」** 对应企业场景的业务闭环：

| 字 | 含义 | 对应业务 |
|---|---|---|
| 汇 | 汇聚、归集 | 信息采集 |
| 批 | 批示、流转 | 流程审批 |
| 呈 | 呈报、呈文 | 数据上报 |
| 览 | 阅览、总览 | 数据分析 |

**组件命名体系**（状态以本仓库实现为准）：

| 层级 | 名称 | 简称 | 定位 | 状态 |
|---|---|---|---|---|
| 项目 / CLI | **Kenvoy** | `kv`（称呼简称，不提供同名命令） | 语义化低代码平台与命令行 | ✅ 已实现，开源（`crates/kenvoy`） |
| 解析器核心 | **KEngine** | `kengine` | DSL 解析引擎：输入语义，输出布局与数据模型 | ✅ 已实现，开源（`crates/kengine`） |
| 云平台 | **KenvoySpace** | `kspace` | 企业数字化云工作空间（控制台 / Dashboard） | 🔒 规划中，未实现，闭源 |
| 生产调度器 | **Kenvoy-Runtime** | `krt` | 生产调度中枢：路由分发、CI/CD、版本控制、日志、多服务协同 | 🔒 规划中，未实现，闭源 |

链路目标是 **Kenvoy CLI → Engine 解析 → Space 承载 → Runtime 调度**；目前只有前两段落地，`kspace` 与 `krt` 是未来规划，不提供接口、仓库或部署方式，任何文档与回复都不得写成已具备。

称呼约定见[称呼与术语](docs/reference/terminology.md)：**kenvoy 项目**专指用本产品 DSL 编写的用户应用，不指本仓库。

## 仓库结构

当前为 Rust 工作区（Rust 1.98 / edition 2024 / resolver 3），四成员，默认成员为两个产品包：

```text
kenvoy/
├── crates/
│   ├── kengine/        # 解析引擎 Library：SWC 解析、Generic IR、语义与模块图
│   │                   #   发布到 crates.io；可独立 check/test，不需要数据库/HTTP/前端
│   ├── kenvoy/         # Kenvoy CLI + 内置前端：DSL、Application IR、SQLite、HTTP、构建与运行
│   │   ├── frontend/   # React + TypeScript + 原生 CSS 的框架前端，构建产物随源包发布
│   │   └── tests/      # 产品独立测试
│   └── kenvoy-tests/   # 跨 Crate 集成测试与真实进程/浏览器夹具（不发布）
├── xtask/              # 工程编排：check/architecture/docs/build/test/browser/dist（不发布）
├── examples/           # 可运行示例：customer-app、uploads-app、options-app、extensions-app
├── docs/               # 文档中心：架构、契约、开发、运维、参考、决策、变更、验证
├── .github/            # 分发工作流与六目标清单（Windows MSVC / Linux musl / macOS）
├── .cargo/             # Cargo 配置
├── AGENTS.md           # 本仓库工作规则（含称呼与术语）
├── PRODUCT.md          # 产品事实
├── DESIGN.md           # 视觉与界面约定
├── Cargo.toml          # Workspace 定义与共享依赖/属性
├── Cargo.lock          # 锁定依赖
└── LICENSE             # MIT
```

依赖方向固定为 `kenvoy → kengine`；`kenvoy-tests` 与 `xtask` 只用于开发验证，产品不依赖它们，且禁止发布。各产品 Crate 保留独立 `tests`。

开源部分为 **Kenvoy CLI（`crates/kenvoy`）+ KEngine（`crates/kengine`）**，两者发布到 [crates.io](https://crates.io) 并提供 GitHub Release 预编译二进制。`KenvoySpace`（`kspace`）与 `Kenvoy-Runtime`（`krt`）是闭源商业产品规划，**不在本仓库**，当前也未实现。

构建产物、依赖目录与运行数据不入库：`target/`、`node_modules/`、`dist/`、`.kenvoy/`、`artifacts/`、`*.sqlite*` 由 `.gitignore` 排除。

开发统一在 `dev`，版本变更提交后自动补本地 `vMAJOR.MINOR.PATCH` 标签；用户批准发布时先逐个推送标签，再合并并推送 `main`。只发布 main 当前版本（包括 PATCH），不补发历史版本；dev 可手动单平台、单架构打包而不发布。规则、钩子启用与 main 手动兜底见[Git 与发布操作](docs/operations/releasing.md)。多渠道发布的当前完成度、待配凭据与首次真实发布步骤见[渠道发布进度](docs/development/channel-rollout-status.md)。

## 核心设计

### JSX/TSX 业务声明

业务描述使用 **JSX/TSX**，入口文件默认为 `src/app.tsx`，默认导出一个 `<app>` 或单个 `<page>`。声明是纯文本、结构化、可被 AI 生成与修改，最终编译为 Application IR，而不是运行期解释坐标或配置：

```tsx
// src/app.tsx
export default (
  <app name="客户管理系统">
    <resource name="customers">
      <text name="name" label="客户姓名" required maxLength={100} />
      <text name="phone" label="联系电话" />
      <number name="age" label="年龄" integer min={0} />
    </resource>

    <page path="/customers" title="客户登记">
      <form resource="customers" mode="create" />
    </page>

    <page path="/customers/list" title="客户查询" permission="admin">
      <table resource="customers" />
    </page>
  </app>
);
```

支持的元素：`app`、`page`、`resource`，八种字段（text/textarea/number/boolean/date/datetime/select/file）、`option`、`form`、`table`、`image`、`action`、`navigation`/`item`/`group`、`override`。属性、父子关系、引用与冲突在 `kenvoy check` 阶段严格校验。页面路径须为规范静态绝对路径，并避开保留命名空间 `/api`、`/admin`、`/_kenvoy`；上例用 `kenvoy check` 实测通过。

模块按职责分文件，静态可分析：

| 文件形态 | 作用 | 边界 |
|---|---|---|
| `*.diu.tsx` | 静态 Schema 组件，默认导出可跨文件组合的声明 | 不接受动态 Props/children，静态求值有深度与步数限额 |
| `*.server.ts` | 服务端函数，供 `action runtime="server"` 使用 | 默认私有，不进入客户端可达图 |
| shared 注册 | 显式共享模块 | 不能借共享标记把私有实现放进浏览器 |
| 普通 `.tsx` | 自定义 React 组件、Hooks、第三方组件库 | 只把实际使用的切片送入客户端 |

### 路由与页面

路由由 `page path` **显式声明**，不是文件系统路由：一个应用可以在同一入口或多文件组合中声明多个页面，导航通过 `navigation`/`item` 描述，`permission` 控制可访问角色。没有 `layout.kenvoy.xml`、`flow.kenvoy.xml`、`publish.kenvoy.xml` 这类文件约定——公共结构由 `group`/`navigation` 与组件复用表达，权限写在声明属性上，审批流不在当前能力范围。

### 目标项目结构

```text
kenvoy 项目根目录/
├── src/
│   ├── app.tsx            # 入口：默认导出 app 或 page
│   ├── *.diu.tsx          # 静态 Schema 组件（可选）
│   ├── *.server.ts        # 服务端函数（可选）
│   └── components.tsx     # 自定义 React 组件（可选）
├── data/app.sqlite        # SQLite 业务库（运行/迁移自动创建，dev 与 run 共用）
├── dist/                  # 构建产物（自动生成，不手工编辑）
├── .kenvoy/               # 客户端工具链缓存与元数据（自动生成）
├── package.json           # 可选：自定义组件依赖，显式 --install-deps 才安装
├── kenvoy.toml            # 项目配置：入口、服务、数据库、前端、运行时、构建
├── .gitignore             # 忽略 dist/、.kenvoy/、node_modules/、*.sqlite*
└── .env                   # 环境变量（可选）
```

配置集中在 `kenvoy.toml`（默认入口 `src/app.tsx`、监听 `127.0.0.1:3000`、SQLite `data/app.sqlite`、输出 `dist`），字段与校验规则见[项目配置](docs/contracts/project-configuration.md)。基础 DSL 运行不需要 Node/npm；只有使用自定义组件库时才需要用户自己的 `package.json` 与显式依赖安装。

### 设计原则

| 原则 | 现状 |
|---|---|
| 语义优先 | 只声明资源、页面、字段、权限与动作；布局、交互、数据联动由 Runtime 与 Application IR 推导 |
| AI 友好 | 纯文本声明 + 严格静态校验，错误在 `kenvoy check` 定位到元素与字段，AI 可稳定生成与修改 |
| 全端自适应 | 同一份声明同时服务桌面与移动宽度，界面由统一 Runtime 渲染，不维护两套页面 |
| 约定集中 | 配置集中在 `kenvoy.toml`，路由由声明给出，不依赖目录命名推断 |
| 关注点分离 | resource/page/action 各司其职；server、shared、client 模块边界在编译期强制 |

### 已放弃的 0.1 设计

早期方案使用 XML（`.kenvoy.xml`）、`apps/` 目录即路由、`layout`/`flow`/`publish` 分文件约定与内置审批流。这些**不属于当前能力**：现行载体是 JSX/TSX，路由由 `page path` 声明，权限是声明属性，审批流未实现。历史方案仅在变更记录中作为当时事实保留，不得据此实现或承诺。

## 当前进度

当前默认 UI 使用 React + TypeScript + 原生 CSS，框架不再依赖 MUI/Emotion。select 支持静态联动和受控跨资源选项查询；系统组件可通过 override 声明替换显示层，自定义组件通过 SDK 继续使用原生资源、字段校验和 API。另支持 SQLite 持久化的多文件字段、附件下载及可显示上传图片/图片 URL 的 Image 组件。用法见[组件与选项契约](docs/contracts/component-bindings.md)、[文件与图片契约](docs/contracts/file-uploads.md)、[联动选项示例](examples/options-app/README.md)和[上传与图片示例](examples/uploads-app/README.md)。

首版基线已完成阶段 00—10，并在 Windows 上通过真实浏览器闭环：通用解析与增量分析、业务 DSL、真实 SQLite 和迁移、管理员认证、受控 HTTP CRUD、前后台、自定义组件、TypeScript/ESM 服务端函数、HMR，以及 create/dev/build/run/check/inspect/migrate。当时 201 项常规 Rust 测试、2 项显式跨工具测试和 90 项前端测试通过；历史命令与边界见[首版验收](docs/verification/0012-final-acceptance.md)，后续变更见[变更索引](docs/changes/README.md)。

产品仍由 crates/kengine（Library）和 crates/kenvoy（Binary，即 Kenvoy CLI）构成，依赖方向为 Kenvoy → KEngine。Workspace 另有专门的 [kenvoy-tests](crates/kenvoy-tests/) 测试成员和根目录 [xtask](xtask/) 工程编排成员，均不发布，产品不依赖它们。各产品 Crate 保留自己的独立 tests。详细状态与证据从 [文档索引](docs/README.md)进入；本仓库工作规则见 [AGENTS.md](AGENTS.md)，称呼约定见[称呼与术语](docs/reference/terminology.md)：**kenvoy 项目**指用本产品 DSL 编写的用户应用，不指本仓库。

内部代码已按 DDD 分层：领域模型与规则、应用用例与静态端口、基础设施、接口及宿主装配；前端同样分离纯规则、API 和展示。自有代码（含测试）每文件硬上限 500 行，目标 400 行内，由 `cargo xtask architecture/check` 检查。目录与依赖规则见[DDD 规范](docs/architecture/ddd-structure.md)，本次行为兼容验证见 [0016](docs/changes/0016-ddd-structure-and-module-split.md)。

## 快速开始

已发布版本（`kenvoy 0.2.0`）安装后即可创建并运行 kenvoy 项目：

```powershell
cargo binstall kenvoy            # 或 cargo install kenvoy --locked
kenvoy create my-app             # 生成含客户登记示例的项目
Set-Location my-app
kenvoy admin init --username admin   # 隐藏输入密码，不提供默认密码
kenvoy dev                       # 开发服务：http://127.0.0.1:3000
```

停止开发服务后启动生产模式，无需先手工 build（有源码且需要更新时会自动构建）：

```powershell
kenvoy check                     # 检查源码、模块、声明、权限与路由，不连接数据库
kenvoy build                     # 生成版本化应用描述、客户端资产与服务端模块，不连接数据库
kenvoy run                       # 加载生产产物并启动
```

`kenvoy --help` 当前提供的命令：

| 命令 | 作用 |
|---|---|
| `create` | 创建包含客户登记示例的项目 |
| `check` | 检查源码、模块、声明、权限与路由，不连接数据库 |
| `inspect` | 查看应用模型或指定资源的注册规则 |
| `migrate` | 预览、应用数据库迁移或读取迁移历史 |
| `admin` | 初始化管理员，或由本机项目所有者创建账号 |
| `build` | 构建版本化应用描述、客户端资产与服务端模块，不连接数据库 |
| `run` | 加载生产产物并启动；有源码且需要更新时自动构建 |
| `dev` | 启动开发服务 |

全局选项：`--project <路径>` 指定项目位置（默认从当前目录向上查找 `kenvoy.toml`）、`--json-logs` 输出结构化日志（不记录密码、Cookie 或表单内容）。

在仓库内开发时可直接运行 CLI 与测试：

```powershell
cargo build --workspace
cargo run -p kenvoy -- --help
cargo xtask test          # 构建正式 CLI 后运行 Workspace 测试
```

基础 DSL 不需要 Node/npm；只有使用自定义组件库时才需要 `kenvoy dev --install-deps` 或 `kenvoy build --install-deps`。

## 安装 0.2.0

0.2.0 已发布至 crates.io 和 GitHub Release；详见[版本日志](docs/changes/CHANGELOG.md)及[执行记录](docs/changes/0019-crates-and-binary-release.md)。安装 CLI，或在 Rust 项目中使用语言库：

```text
cargo binstall kenvoy --version 0.2.0
cargo install kenvoy --version 0.2.0 --locked
cargo add kengine@0.2.0
```

源包自带通用前端发行资产，安装 CLI 不需要在 Cargo 缓存目录手动运行 npm。预编译分发覆盖 Windows MSVC、Linux musl、macOS 的 x86_64/aarch64；归档与 SHA256 来自 [GitHub Release](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.2.0)。发布工具、Zig 交叉编译及验收边界见[发布操作](docs/operations/releasing.md)，包级说明见 [Kenvoy](crates/kenvoy/README.md) / [KEngine](crates/kengine/README.md)。

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
