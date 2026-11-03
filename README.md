# Kenvoy（汇批呈览）

> 面向 AI 时代的语义化低代码平台：**写语义，出全端。**

[![crates.io](https://img.shields.io/crates/v/kenvoy.svg)](https://crates.io/crates/kenvoy)
[![GitHub release](https://img.shields.io/github/v/release/zerobiubiu/kenvoy)](https://github.com/zerobiubiu/kenvoy/releases)
[![docs.rs](https://docs.rs/kengine/badge.svg)](https://docs.rs/kengine)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Kenvoy 把 TypeScript/JSX 声明解释为具有页面、受控 API、管理后台和 SQLite 持久化的完整应用。开发者只需描述业务对象、字段、页面、权限与动作，布局、交互、数据校验和前后端接线由运行时完成。

KEngine 是其中独立、通用的 Rust 源码解析库，提供 JS/TS/JSX/TSX 解析、通用 IR、模块图、语义分析与诊断，不包含 Web 业务语义。

## 特性

- **语义化声明**：`resource`、`page`、`form`、`table`、`action` 等元素描述业务，严格静态校验引用、权限与路由。
- **单一业务模型**：同一份声明驱动页面、管理后台、受控 HTTP API 与 SQLite 表结构，不为每个资源手写 CRUD。
- **真实持久化**：SQLite 文件数据库、追加式正式迁移链、逐步事务与 checksum 确认；开发与生产共用同一数据路径，不因切换模式重置。
- **内置管理后台**：登录、会话、资源查询、分页、排序、编辑与删除，复用同一权限模型。
- **可扩展前端**：原生 React + TypeScript + CSS；支持组件重载、自定义组件、联动选项、多文件上传与图片展示。
- **原生国际化**：始终启用；集中译文、语言代码/别名与明确显示名、设备语言及兜底、JSON Schema 校验、Intl 格式化和 `useI18n()`，无需应用 Provider。
- **受限 JavaScript**：服务端函数与 Action 运行在 QuickJS 中，客户端代码按依赖切片，不把服务端私有模块带入浏览器。
- **工程闭环**：`create` / `check` / `inspect` / `i18n` / `migrate` / `admin` / `build` / `run` / `dev`，含受控 Vite/HMR 与版本化产物。

## 快速开始

### 安装

```bash
# 预编译二进制（推荐）
cargo binstall kenvoy

# 或从源码编译
cargo install kenvoy --locked
```

在 Rust 项目中单独使用语言库：

```bash
cargo add kengine
```

### 创建并运行

```powershell
kenvoy create my-app
Set-Location my-app
kenvoy admin init --username admin   # 隐藏输入密码，不提供默认密码
kenvoy dev                           # http://127.0.0.1:3000
```

访问 `/customers` 登记数据；登录 `/admin/customers` 查询、编辑和删除。匿名只有声明允许的表单提交权限。

当前 dev 源码为 Kenvoy CLI **0.4.0**（尚未发布），KEngine 保持 **0.2.0**；安装命令获取已发布版本，不代表本次功能已上线。create 生成 languages 默认配置、根 language.schema.json 与 translations.json；`kenvoy i18n` 更新 Schema 并在缺失时初始化译文。dev/run 必须先校验集中译文，语言不能关闭。配置、自动键及 SDK 见[原生语言契约](docs/contracts/i18n.md)，完整例子见 [i18n-app](examples/i18n-app/README.md)。


### 生产运行

```powershell
kenvoy check    # 检查源码、模块、声明、权限与路由，不连接数据库
kenvoy build    # 生成版本化应用描述、客户端资产与服务端模块
kenvoy run      # 加载生产产物并启动；有源码且需要更新时自动构建
```

基础 DSL 运行不需要 Node/npm；只有使用自定义组件库时才需要 `kenvoy dev --install-deps` 或 `kenvoy build --install-deps`。

`create` 已固化模板的初始迁移。之后改变数据库结构，先 `kenvoy migrate generate --description "变更说明"`，审查候选后 `kenvoy migrate seal`，再构建或启动；危险变更还需显式规则、精确确认和一致性备份。`run` 只执行已固化步骤，空库也执行完整链，不在启动时临时生成方案。

旧配置和旧库不会自动接入：配置必须显式声明 `project.min_cli_version`、`project.format_version`，数据库通过只读报告和 checksum 确认选择真实基线。一次性开发可用 `dev --snapshot dev-snapshots/try.sqlite`；快照不可直接用于生产。步骤和恢复边界见[迁移操作](docs/operations/migration-design.md)。

## 声明示例

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

页面路径须为规范静态绝对路径，并避开保留命名空间 `/api`、`/admin`、`/_kenvoy`。完整元素、字段约束与模块边界见[已实现声明语法](docs/contracts/dsl-specification.md)。

## 文档

| 入口 | 内容 |
|---|---|
| [文档中心](docs/README.md) | 全部文档索引、规范与维护要求 |
| [DSL 语法](docs/contracts/dsl-specification.md) | 元素、字段、权限、Action 与模块边界 |
| [项目配置](docs/contracts/project-configuration.md) | `kenvoy.toml` 默认值、路径与校验规则 |
| [原生国际化](docs/contracts/i18n.md) | 强制 languages、集中译文、代码/别名、Schema、设备语言与无源码交付 |
| [CLI 使用](docs/operations/cli-design.md) | create/check/i18n/migrate/admin/dev/build/run |
| [发布与渠道](docs/operations/releasing.md) | 版本标签、发布流程与多渠道状态 |
| [可运行示例](examples/README.md) | customer-app、uploads-app、options-app、extensions-app、[i18n-app](examples/i18n-app/README.md) |

## 仓库结构

```text
crates/
  kengine/        # 解析引擎 Library（发布到 crates.io）
  kenvoy/         # CLI + 内置前端（发布到 crates.io）
  kenvoy-tests/   # 跨 Crate 集成测试与进程/浏览器夹具（不发布）
xtask/            # 工程编排（不发布）
docs/             # 文档中心
examples/         # 可运行示例项目
.github/          # 发布与分发工作流
```

依赖方向固定为 `kenvoy → kengine`；测试与工程成员不进入产品依赖闭包。

## 开发

需要 Rust 1.98+、原生 C 构建环境，以及 Node 24.22+/npm（构建前端）。

```powershell
npm ci --ignore-scripts --prefix crates/kenvoy/frontend
npm run build --prefix crates/kenvoy/frontend
cargo build -p kenvoy --locked
```

验证入口：

```powershell
cargo xtask check      # 架构、目录、规模与文档检查
cargo xtask test       # 构建正式 CLI 后运行 Workspace 测试
cargo xtask browser    # 显式运行真实浏览器用例
```

开发在 `dev` 分支进行；版本变化自动维护本地版本标签，发布时合并到 `main` 并触发当前版本发布。规则见[发布操作](docs/operations/releasing.md)，工程细节见[工程流程](docs/development/engineering-workflow.md)。

## 分发状态

当前已发布：

- [crates.io](https://crates.io/crates/kenvoy)：`kenvoy` CLI 与 `kengine` 库
- [GitHub Releases](https://github.com/zerobiubiu/kenvoy/releases)：Windows MSVC、Linux musl、macOS 的 x86_64 / ARM64 预编译 ZIP 与 SHA256

npm、Homebrew、Scoop、WinGet、Chocolatey、AUR 以及 APT/RPM/Alpine/Arch 仓库已配置流水线但**尚未上线**，请勿按未验证方式安装；进度与待配凭据见[渠道发布进度](docs/development/channel-rollout-status.md)。

## 参与贡献

- 提交前先读 [AGENTS.md](AGENTS.md) 与[文档中心](docs/README.md)。
- 一个逻辑变更一个提交，中文提交信息写清“做了什么、为什么”。
- 功能变更同步测试、契约文档与变更记录；不提交密钥、构建产物或业务数据。

## 许可证

[MIT](LICENSE)。Kenvoy 读作 `/ˈkɛn.vɔɪ/`，中文名「汇批呈览」取汇聚、批示、呈报、阅览之意。
