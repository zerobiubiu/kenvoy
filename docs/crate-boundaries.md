# 双 Crate 职责与依赖边界

状态：现行。Workspace 恰有两个成员，Kenvoy 通过本地路径依赖消费 KEngine 公共 API；本篇维护职责约束，实际检查证据见[最终验收](verification/0012-final-acceptance.md)。

## 固定边界

| 能力 | kengine Library | kenvoy Binary |
|---|---|---|
| JS/TS/JSX/TSX 语法、位置、通用诊断 | 负责 | 调用公共 API |
| 模块图、作用域、符号与引用 | 通用语言关系 | 解释模块在应用中的用途 |
| JSX Element、Fragment、spread、动态表达式 | 无业务含义地保留 | 识别 DSL 并静态求值允许的声明 |
| Generic IR | 定义、版本化、序列化 | 消费 |
| Application IR | 禁止 | 定义并驱动全部 Runtime |
| page/resource/field/admin/permission | 只可当普通 JSX 名称 | 定义业务语义 |
| React/原生 CSS、CSR、构建/HMR | 禁止 | 前端适配 |
| Axum/Tokio、认证、API | 禁止 | HTTP 与生命周期 |
| rusqlite、Schema、迁移、CRUD | 禁止 | 数据库适配 |
| TS 去类型和服务端 JavaScript 执行 | 不提供 Kenvoy 执行流水线 | SWC 转换适配与 rquickjs 执行 |
| Kenvoy 配置、CLI、项目发现 | 禁止 | 负责 |

KEngine 可以解析任意名为 page 的 JSX 标签，也可以通用解析 package.json/tsconfig 的模块定位字段；不能据此创建 HTTP 路由、数据库或读取 kenvoy.toml。Kenvoy 不绕过公共 API 访问 KEngine 私有 SWC 节点。

## Cargo 约束

Workspace 只保留 `crates/kengine` 和 `crates/kenvoy`。KEngine 仅 lib，无 main.rs、bin target 或业务 feature；Kenvoy 仅面向用户的 Binary，不为模块化再建 library target 或 Rust 包。单向依赖为 `kenvoy → kengine`。

根 workspace members 已改为明确两项，Kenvoy manifest 已加入 `kengine = { path = "../kengine" }`。KEngine 的正式依赖仅为通用 SWC/serde/thiserror；HTTP、SQLite、JS 宿主和前端探针依赖只在 Kenvoy 或非 Rust 探针目录。测试 Target 不属于额外面向用户 Binary。

Rust 的第三方依赖包不属于本工作区成员，“只有两个 Crate”限制针对项目自有 Workspace Crate，不意味着禁止 Cargo 依赖成熟第三方库。探针放进现有两个包的测试或非 Cargo 独立资料中，不新建第三个探针包。

## 公共 API 与模型隔离

KEngine 的公共结果只含自有稳定类型：ProgramIR、ModuleIR、ElementIR、ExpressionIR、Symbol、Reference、ModuleGraph、Diagnostic。内部可保留 SWC AST 并用于转换，不能将它作为长期公共契约或序列化协议。

Kenvoy 内部 `application`、`dsl`、`database`、`http`、`frontend`、`javascript` 按真实功能逐步建立。设计中的模块名称不要求阶段 00 创建空目录。具体适配器使用内部模块隔离，首版不为未来扩展实现多套后端。

## 验证与防回归

阶段 01 起检查 Cargo metadata：成员数为 2、目标类型符合约束、Kenvoy 存在路径依赖、KEngine 不依赖 Kenvoy。依赖图同时检查 KEngine 没有 Axum、rusqlite、React 相关业务依赖。

代码评审检查语义边界，不能只通过关键词扫描判定：KEngine 的通用 JSX 测试允许出现任意标签名。业务 schema、路由、安全策略和运行时行为必须在 Kenvoy 测试中验证。KEngine 单独 cargo check/test 应可运行，不需数据库、HTTP、浏览器或 Kenvoy 配置。

相关决策：[ADR 0001](adr/0001-two-crate-architecture.md)、[ADR 0002](adr/0002-kengine-generic-parser.md)。
