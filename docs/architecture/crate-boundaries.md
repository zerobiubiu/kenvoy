# 产品 Crate 与工程成员的职责边界

状态：参考（现状文档）。产品由 KEngine Library 和 Kenvoy Binary 构成；Workspace 另有不发布的测试成员和 xtask。Kenvoy 通过本地路径依赖消费 KEngine 公共 API。此次工程组织调整见 [ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)，历史产品验收见[最终验收](../verification/0012-final-acceptance.md)。

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

Workspace 明确列出四个成员：

| 成员 | 目标与用途 | 发布/依赖约束 |
|---|---|---|
| `crates/kengine` | 通用 Library | 无 Binary、无业务依赖 |
| `crates/kenvoy` | 唯一产品 Binary | 通过公共 API 依赖 KEngine |
| `crates/kenvoy-tests` | 测试支撑 Library 与标准集成测试 | `publish = false`，仅测试使用产品公共接口和真实 CLI |
| `xtask` | 内部工程 Binary | `publish = false`，只做检查、构建和测试编排 |

产品不得反向依赖测试成员或 xtask。KEngine 的正式依赖仅为通用 SWC/serde/thiserror，Kenvoy 保持 `kengine = { path = "../kengine" }`；不为测试暴露 Kenvoy 内部 Library。跨 Crate 场景通过公共 KEngine API 和已构建 Kenvoy CLI 联合验证。

默认成员仍为两个产品包，`cargo test --workspace` 包括工程成员。每个产品 Crate 的独立 tests 继续保留，跨 Crate 测试不替代其单元与模块测试。第三方 Cargo 包不计入自有成员清单。

## 公共 API 与模型隔离

KEngine 的公共结果只含自有稳定类型：ProgramIR、ModuleIR、ElementIR、ExpressionIR、Symbol、Reference、ModuleGraph、Diagnostic。内部可保留 SWC AST 并用于转换，不能将它作为长期公共契约或序列化协议。

Kenvoy 内部按 `domain/application/infrastructure/interfaces/bootstrap` 组织；KEngine 采用对应语言域分层并保留公开根门面。领域只含模型和值规则，应用经真实静态端口使用基础设施，具体适配器由外层装配；前端采用 `app/domain/application/infrastructure/presentation`。详见[DDD 目录规范](ddd-structure.md)，不保留迁移前内部根路径的兼容层，不为未来扩展建立空适配器。

## 验证与防回归

`cargo xtask architecture` 检查真实 Cargo metadata：四个成员及各自目标类型、产品默认成员、测试/工程成员禁止发布、Kenvoy 路径依赖、产品不依赖工程成员，以及 KEngine 普通依赖闭包不含 Axum、rusqlite 等业务宿主依赖。同一入口还通过 AST 检查内部目录与层依赖、生产通配导入、HTTP 仓储泄露及前端 Context/SDK 注册表回引，并对自有代码（含测试）执行每文件 500 物理行硬上限。

代码评审检查语义边界，不能只通过关键词扫描判定：KEngine 的通用 JSX 测试允许出现任意标签名。业务 schema、路由、安全策略和运行时行为必须在 Kenvoy 测试中验证。KEngine 单独 cargo check/test 应可运行，不需数据库、HTTP、浏览器或 Kenvoy 配置。

相关决策：[ADR 0001](../decisions/0001-two-crate-architecture.md)、[ADR 0002](../decisions/0002-kengine-generic-parser.md)、[ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)、[ADR 0012](../decisions/0012-ddd-layers-and-module-boundaries.md)。0011 替代 0001 的 Workspace 成员数量限制，保留两个产品层的职责与依赖方向；0012 约束其内部实现组织。
