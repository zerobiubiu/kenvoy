# 阶段 10：第一版基础框架最终验收

状态：已执行；结论：本轮 Windows 环境下第一版基础闭环通过；日期：2026-11-01。

## 交付范围

唯一用户 Binary 为 `target/debug/kenvoy.exe`，最终复验文件 SHA-256：

```text
8656D3E3F5D6324C3FC98DA0990B42BBF6BA21855F8F112FCF38080F10701B68
```

Workspace 恰有 KEngine Library 和 Kenvoy Binary，依赖为 `kenvoy → kengine`。两个包版本仍为 0.1.0；未提交、推送、打标签或发布。源码构建及运行步骤见[根 README](../../README.md)。两个可运行项目为[基础客户应用](../../examples/customer-app/src/app.tsx)及[扩展客户应用](../../examples/extensions-app/README.md)。

所有浏览器脚本通过 CLI 创建唯一临时项目，执行真实 HTTP 与 SQLite。没有用内存数据、静态示意页面或模拟 API 代替验收，也没有修改用户业务数据库。CLI 的 run 与 dev 均负责实际启动；生产不要求另外启动数据库或 Node 服务。

## 必需能力映射

| 验收能力 | 证据与结果 |
|---|---|
| 创建项目、一条命令开发 | CLI 集成测试与所有 browser fixture 实际 create→dev |
| TSX、通用 IR、模块与符号 | 66 项独立 KEngine 测试通过；无业务依赖 |
| App/Page/Resource/Field、静态组合 | DSL 单测与跨文件 Schema Component 真实页面 |
| 真实 SQLite 初始化、Schema 与迁移 | 数据库/CLI 测试；safe addition 与 destructive rejection 浏览器回归 |
| 页面字段和前后端校验 | forms/fields；姓名必填/100字、整数/min、七类字段及共享68向量 |
| 张三、电话、年龄真实提交 | forms E2E，后端写库并返回真实记录 |
| 后台认证、搜索/筛选/排序/分页 | admin E2E；匿名 API 拒绝、普通用户权限另由 HTTP 验证 |
| 查看、新增、编辑、删除 | admin E2E，与前台共用资源/数据库 |
| 持久化 | dev 重启、真实 Ctrl+C 后重启、dev→run 共用文件均保留记录 |
| 自定义 React、Hooks/Props、MUI/第三方/CSS | extensions E2E，clsx、barrel 副作用与内联常量捕获 |
| TS 服务端函数 | 真实页面调用经过 TS 去类型的跨文件 ESM Promise 函数，返回折扣 0.8 |
| 客户端/服务端隔离 | client 资产无私有标记，server 文件 URL 为404，异常不暴露私有内容 |
| 开发 HMR | 修改组件与 CSS 保持计数，语法错误保留有效镜像，修复恢复 |
| 配置/Schema 失败恢复 | watcher 新增配置恢复原文清 Error、保留草稿及 generation；危险迁移保留活动清单 |
| build、run 自动构建、产物部署 | production E2E：不预build→run、源码变化重建、无源码/无Node运行、账号命令可用 |
| 退出与资源释放 | 独立 PTY Ctrl+C，含活动 WebSocket 的关闭及重启验证，见[阶段09](0011-cli-lifecycle.md) |
| inspect/check/migrate 与诊断 | CLI 集成测试、示例 check、JSON 源码诊断、真实迁移历史/确认/回滚 |

## 最终执行命令

Windows、Rust/Cargo 1.99.1、Rust 1.98.0、Node 24.22.0、npm 12.2.0、Chromium 156.0.8078.4。Cargo 进程使用 `CARGO_INCREMENTAL=0`，不修改全局设置。以下全部实际执行并观察到退出 0：

| 命令 | 实际结果 |
|---|---|
| `cargo check --workspace --all-targets --offline --locked` | 全目标检查通过 |
| `cargo test --workspace --offline --locked` | 201 项通过，2 项默认 ignored；KEngine 66、Kenvoy 单元106、其余集成29；看门狗子进程不重复计数 |
| `cargo test --workspace --offline --locked -- --ignored` | 两项均通过：真实自定义构建/开发缓存26.73秒；Axum/React/SQLite浏览器5.96秒 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 严格检查通过，无抑制新告警 |
| `cargo build -p kenvoy --bin kenvoy --offline --locked` | 原生 CLI 构建通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo +1.98.0 check --workspace --all-targets --offline --locked` | 最终修订5.18秒，退出0；不是最低版本完整运行矩阵 |
| `cargo test -p kengine --offline --locked` | 独立66项通过 |
| `npm run check --prefix crates/kenvoy/frontend` | 严格 TS 检查通过 |
| `npm test --prefix crates/kenvoy/frontend` | 90项通过 |
| `npm run build --prefix crates/kenvoy/frontend` | 真实React产物生成；主JS约594kB，保留500kB提示 |
| `node crates/kenvoy/frontend/scripts/forms-e2e.mjs` | Chromium真实表单、校验、SQLite、登录、桌面/移动通过 |
| `node crates/kenvoy/frontend/scripts/fields-e2e.mjs` | 七类控件、Unicode、false默认值、显式时区、真实数据通过 |
| `node crates/kenvoy/frontend/scripts/admin-e2e.mjs` | 完整后台CRUD、查询、详情、空值保留、重启和移动操作通过 |
| `node crates/kenvoy/frontend/scripts/extensions-e2e.mjs` | 自定义组件、服务端函数、隔离、真实HMR/错误恢复通过 |
| `node crates/kenvoy/frontend/scripts/watcher-e2e.mjs` | 原生监听、配置恢复、完整诊断、候选及数据库保护通过 |
| `node crates/kenvoy/frontend/scripts/production-e2e.mjs` | 自动构建、无源码/Node、账户管理、开发接口404；此次产物启动约2195ms，仅为夹具观测 |
| `node tools/tests/http-cli.mjs` | 20项真实HTTP断言通过 |
| `kenvoy --project examples/customer-app check`、扩展示例同命令 | 1资源/1页面；扩展示例另有1动作，均通过 |
| `node tools/verify-architecture.mjs` | 两成员、单向依赖，KEngine正常依赖闭包104个包 |
| `node tools/verify-docs.mjs` | 64份Markdown、286条本地链接通过 |
| `git diff --check` | 退出0；仅有既存LF/CRLF转换提示 |

常规和显式 Rust 原始日志分别保留在本地 `artifacts/final/rust-tests.log`、`rust-explicit-tests.log`；产物目录已被 Git 忽略。最终进程核对未发现 Kenvoy 或测试项目 Node 残留；两个示例均未生成业务 SQLite。Impeccable detector 命令为 `impeccable.cmd detect --json crates/kenvoy/frontend/src crates/kenvoy/frontend/index.html`，退出0、原始输出 `[]`；它不替代浏览器验收。

## 失败修复与残余边界

历史失败没有删除：Windows异步栈、首次优化依赖404、HMR Context循环、路径隔离、配置恢复、资产校验分类及末轮Clippy模块顺序等，见[阶段09报告](0011-cli-lifecycle.md)。本轮修复后才执行上述通过门禁。

本次完成基础框架，不宣称跨平台发行、所有浏览器/设备、完整 TypeScript 类型检查或 Node兼容。Schema必须静态；重命名只检测并阻断；不可信JavaScript拒绝执行。文件/关系字段、资源Host API、操作系统沙箱和多框架不在首版实现内。断电、磁盘满、网络盘、构建/发布中途Ctrl+C、长时负载尚未全面验收；历史构建目录暂不自动回收。

两个已关闭的手动退出夹具清理被自动审批审查拒绝，原因为 `blocked by policy`，未提供更详细说明；未绕过，路径和过程见[阶段09](0011-cli-lifecycle.md)。这不影响服务、数据库或浏览器验收，但临时文件仍占用磁盘。
