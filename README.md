# Kenvoy

**汇批呈览 · 面向 AI 时代的语义化低代码平台**

开发者用专有的 DSL（领域特定语言）描述业务，Kenvoy 自动解析并生成桌面/移动端前端页面，同时关联数据模型与后台逻辑。

> 一句话定位：**写语义，出全端。**

## 命名来历

**Kenvoy** 读作 `/ˈkɛn.vɔɪ/`，是一个生造词（非现成英文单词，词根可辨）：

| 词根 | 来源 | 含义 |
| :--- | :--- | :--- |
| **Ken** | 古英语 | 「知悉 / 看见 / 视野」，对应**览**（总览全局） |
| **voy** | Voyage（旅程/流转） | 「流转」，对应**汇 / 批 / 呈**（信息汇聚与流程流转） |

中文名 **「汇批呈览」** 对应 ToB 企业场景的完整业务闭环：

| 字 | 含义 | 对应业务 |
| :--- | :--- | :--- |
| **汇** | 汇聚 / 归集 | 信息采集 |
| **批** | 批示 / 流转 | 流程审批 |
| **呈** | 呈报 / 呈文 | 数据上报 |
| **览** | 阅览 / 总览 | 数据分析 |

> **Kenvoy —— 知悉一切，流转万物。**

## 为什么做 Kenvoy

现有低代码平台大多基于**拖拽式**操作，存在两个根本问题：

1. **AI 无法操作**——拖拽是 GUI 交互，AI 无法通过自然语言生成拖拽操作序列；
2. **语义丢失**——拖拽生成的是坐标和配置，丢失了业务语义，AI 无法理解与维护。

Kenvoy 的解法是**语义化 DSL**：AI 只输出极简的 DSL 描述，核心引擎负责理解语义、匹配预制组件、输出可部署的前后台。为 AI 时代提供一套确定性的「语义接口」。

## 组件体系

| 组件 | 英文名 | 简称 | 定位 | 开源 |
| :--- | :--- | :--- | :--- | :--- |
| **命令行工具** | Kenvoy | `kv` | 开发者入口：`init` / `dev` / `build` / `deploy`，本地实时解析渲染预览 | **开源** |
| **解析器核心** | Kenvoy-Engine | `kengine` | DSL 解析引擎，输入语义输出布局描述与数据模型 | **开源** |
| **云工作空间** | KenvoySpace | `kspace` | 企业级云平台：控制台、Dashboard、团队协作 | 闭源 |
| **生产调度器** | Kenvoy-Runtime | `krt` | 生产环境调度中枢：路由分发、CI/CD、版本控制、日志管理、多服务协同 | 闭源 |

**开源部分**：Kenvoy CLI + Kenvoy-Engine 位于同一个 Rust Workspace，发布到 [crates.io](https://crates.io)。开发者可以本地使用 `kv` 命令，也可以将 `kengine` 作为库集成到自己的 Rust 项目中。

**闭源部分**：KenvoySpace + Kenvoy-Runtime 为云平台侧商业组件，提供企业级生产环境能力。

## DSL 设计

Kenvoy DSL 采用 **XML** 语法（`.kenvoy.xml`），目录结构参照 Next.js 风格，**文件系统即路由**：

- **layout**：布局定义，可覆盖继承——匹配优先级「同级 > 父级 > 父父级 > … > 根 layout」；`apps/` 根下必须有 `layout.kenvoy.xml`
- **page**：页面，路径即 `apps/` 下的目录路径，自动关联最近的 layout
- **flow**：流程（审批流 / 工作流），只出现在 page 同级，由 page 显式引入
- **publish**：发布权限，只出现在 page 同级，由 page 显式引入

### 自定义组件

- **页面私有**：`page.kenvoy.xml` 内与 `<Content>` 平级定义 `<Component name="...">`，仅本页可用
- **全局 / 应用私有**：`module/` 目录下的 `*.kenvoy.xml`（根元素 `<Component name="...">`），全项目或应用内复用
- **使用**：直接写 `<组件名 attr="值">…</组件名>`；实例属性通过 `{{名}}` 绑定到定义体内（Text / Section 标题 / StatCard / Field 默认值等），实例子元素注入定义体内的 `<Slot/>`

### 目标项目结构

```text
apps/                   # 所有应用（根下有仪表盘，应用再分页面）
├── dashboard/          # 应用工作台
├── <应用>/
│   ├── layout.kenvoy.xml
│   ├── module/         # 应用私有自定义组件
│   └── <页面>/
│       ├── page.kenvoy.xml
│       ├── flow.kenvoy.xml
│       ├── publish.kenvoy.xml
│       └── layout.kenvoy.xml   # 可选，覆盖父级
module/                 # 全局通用自定义组件
auth/                   # 用户 / 角色 / 通讯录 / 部门（系统原生）
i18n/                   # 翻译对应表（zh / eng）
public/                 # 静态资源（不可运行时写入，上传走 S3/SQL）
dist/                   # 构建产物
docs/                   # 项目文档
packages/               # 外部依赖库
kenvoy.toml             # 项目配置
.kenvoy/                # 本地元数据 / 缓存
.env                    # 环境变量
```

详细约定见 [docs/03-project-structure.md](./docs/03-project-structure.md)。

## 代码仓库现状

Rust 1.98 / edition 2024 / resolver 3，单 Workspace 多 crate：

```text
kenvoy/
├── crates/
│   ├── kengine       # 解析器核心（纯库，无 I/O）：XML 解析 → IR、表达式引擎、表单校验、审批流状态机、自定义组件解析
│   ├── kengine-html  # SSR 渲染：IR → HTML（Tabler / Bootstrap 5，kv-* 语义类，移动端断点适配）
│   ├── kengine-db    # PostgreSQL 18+ 数据层：自动建表迁移、数据访问鉴权（DataFilter / 字段写权限）
│   ├── kengine-s3    # S3 对象存储抽象（兼容 S3 API 的任何服务，MinIO 联调通过）
│   └── kenvoy        # CLI：init / dev / build / deploy / user / role / contact / department
├── examples/leave-app    # 请假审批示例项目
└── docs/                 # 设计文档（IR 契约、示例计划、项目结构、存储架构、变更记录）
```

## 快速开始

```bash
cargo check --workspace
cargo test --workspace

# 初始化项目，然后启动开发服务器实时预览
cargo run -p kenvoy -- init my-app
cargo run -p kenvoy -- dev

# 用户 / 角色 / 通讯录 / 部门管理（连接 PostgreSQL 时生效）
cargo run -p kenvoy -- user add --username alice --password '***' --display-name 张三
cargo run -p kenvoy -- contact add --name 李四 --user alice
```

文档索引见 [docs/README.md](./docs/README.md)。

## License

开源部分（`kengine` + `kenvoy`）采用 **MIT** 许可证。

KenvoySpace 与 Kenvoy-Runtime 为闭源商业产品。

## Workspace

```text
crates/
├── kengine/         # 解析核心 + HTML 渲染（feature `html`），可面向 WASM
└── kenvoy/         # CLI / dev 服务器 / 静态构建，含 db + s3 接线
```

## Features

| crate | feature | 作用 | 依赖 |
| :--- | :--- | :--- | :--- |
| `kengine` | `html`（默认开） | SSR HTML 渲染 + 内嵌 Tabler 资产 | — |
| `kenvoy` | `db`（默认开） | PostgreSQL 18+ 持久化（sqlx + argon2 + sha2） | sqlx / argon2 / chrono / sha2 |
| `kenvoy` | `s3`（默认开） | S3 对象存储抽象 | rust-s3 / uuid |

裁剪示例：
```bash
# 纯解析器（未来面向 WASM 边缘渲染）
cargo build -p kengine --no-default-features

# 仅 CLI 接线（不含 db/s3）
cargo build -p kenvoy --no-default-features

# 默认 build（完整运行时）
cargo build --workspace
```

## 快速开始

```bash
cargo check --workspace
cargo test --workspace
cargo run -p kenvoy -- --help

# 项目演示（需要 PostgreSQL 18+ 与 MinIO 容器，见 examples/leave-app/kenvoy.toml）
cargo run -p kenvoy -- dev --dir examples/leave-app
cargo run -p kenvoy -- build --dir examples/leave-app
```