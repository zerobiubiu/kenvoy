# 架构与模块设计

状态：参考；治理：现状文档。

[返回文档中心](../README.md)

## 目录职责

本目录解释 KEngine 与 Kenvoy 的总体结构、模块职责、依赖方向及设计取舍，串联从源码分析、业务声明到应用运行和发布的流程。两个产品 Crate 与内部测试、工程成员的职责分别说明，不把工程成员当作新增产品层。

精确 API、IR 格式、DSL 属性、配置默认值和 SDK 约定统一由[接口与数据契约](../contracts/README.md)维护；设计文档通过链接引用这些规范，不复制完整契约。历史取舍见[决策索引](../decisions/README.md)，已执行的命令、结果和覆盖限制见[验证索引](../verification/README.md)。

## 文档清单

以下共 13 篇主题文档，均为可持续维护的参考现状文档。“参考”不等于全部能力已验收；已实现、设计中及待验证范围以各篇正文和原始证据为准。

| 文档 | 状态 | 内容概览 |
|---|---|---|
| [总体架构与现状](architecture.md) | 参考 | 产品分层、组件职责、两层 IR、生命周期一致性及历史基线。 |
| [Crate 与工程成员边界](crate-boundaries.md) | 参考 | KEngine Library、Kenvoy Binary、测试成员与 xtask 的依赖和职责约束。 |
| [当前技术决策与依据](technical-decisions.md) | 参考 | 已采用技术的原因、代价及验证边界；历史决策正文仍在 ADR 中。 |
| [KEngine 实现设计](kengine-design.md) | 参考 | 通用解析、语言语义、模块链接和增量会话；不承载业务解释。 |
| [编译管线与通用解析设计](compiler-design.md) | 参考 | 源码到 Generic IR、DSL 和运行产物的管线、结果所有权、诊断与缓存边界。 |
| [Kenvoy 实现设计](kenvoy-design.md) | 参考 | 项目加载、业务模型、存储、认证、HTTP 和构建协调的宿主分工。 |
| [DSL 与 Application IR 设计](dsl-design.md) | 参考 | 静态声明、资源命名、组件分类、权限和两层模型的设计原则。 |
| [声明解释与运行时分工](interpreter-design.md) | 参考 | 静态业务分析、资源与页面解释、Action、错误和候选更新的责任划分。 |
| [应用运行时与生命周期设计](runtime-design.md) | 参考 | 同代快照、HTTP、dev/build/run、候选发布、失败恢复和资源关闭。 |
| [默认 React Runtime](frontend-runtime.md) | 参考 | 通用 React/原生 CSS 页面与后台、客户端构建、组件扩展和开发刷新。 |
| [TypeScript 扩展与 JavaScript Runtime](javascript-runtime.md) | 参考 | TS 转换、ESM 模块、受限 QuickJS 执行、JSON 边界和 Action 发布。 |
| [SQLite、Schema 与迁移设计](database-design.md) | 参考 | 数据库线程、字段存储、Schema Diff、确认、备份与模型替换。 |
| [认证、授权与 HTTP 安全](security-design.md) | 参考 | 威胁模型、账户与 Session、服务端授权、请求安全、产物与开发代理边界。 |

## 推荐阅读顺序

1. 先读[总体架构](architecture.md)、[职责边界](crate-boundaries.md)和[技术决策](technical-decisions.md)，明确产品与工程组织及单向依赖。
2. 沿语言层阅读 [KEngine](kengine-design.md)和[编译管线](compiler-design.md)，再转到 [KEngine API](../contracts/kengine-api.md)与 [Generic IR](../contracts/generic-ir.md)核对调用和格式。
3. 沿业务层阅读 [Kenvoy](kenvoy-design.md)、[DSL 设计](dsl-design.md)和[解释器分工](interpreter-design.md)，再查 [DSL 规范](../contracts/dsl-specification.md)与 [Application IR](../contracts/application-ir.md)。
4. 阅读[运行时](runtime-design.md)，按关注点进入[数据库](database-design.md)、[安全](security-design.md)、[前端](frontend-runtime.md)及 [JavaScript](javascript-runtime.md)专题；配置和扩展分别查[项目配置](../contracts/project-configuration.md)、[组件绑定与 SDK](../contracts/component-bindings.md)。
5. 需要判断完成范围时回到[验证索引](../verification/README.md)；后续计划查[实施路线](../development/development-roadmap.md)，命令操作查[运维入口](../operations/README.md)，不从设计说明推导验收结论。

## 相关代码与配置

| 设计关注点 | 当前实现或配置入口 |
|---|---|
| Workspace 与产品入口 | [根 Cargo.toml](../../Cargo.toml)、[KEngine 公共入口](../../crates/kengine/src/lib.rs)、[Kenvoy CLI 入口](../../crates/kenvoy/src/main.rs)、[跨 Crate 测试成员](../../crates/kenvoy-tests/)、[xtask](../../xtask/) |
| 通用解析与 IR | [解析器](../../crates/kengine/src/parser.rs)、[Generic IR 类型](../../crates/kengine/src/ir.rs)、[增量分析](../../crates/kengine/src/incremental.rs) |
| 静态业务分析 | [DSL 分析器](../../crates/kenvoy/src/dsl/analyzer.rs)、[静态求值器](../../crates/kenvoy/src/dsl/static_eval.rs)、[Application IR 类型](../../crates/kenvoy/src/application.rs) |
| 项目与生命周期 | [项目配置](../../crates/kenvoy/src/project.rs)、[生命周期协调](../../crates/kenvoy/src/lifecycle.rs)、[版本化产物](../../crates/kenvoy/src/artifact.rs) |
| 存储、HTTP 与认证 | [数据库模块](../../crates/kenvoy/src/database/)、[HTTP 模块](../../crates/kenvoy/src/http/)、[认证模块](../../crates/kenvoy/src/auth.rs) |
| 浏览器运行时与工具链 | [React Runtime](../../crates/kenvoy/frontend/src/Runtime.tsx)、[前端包配置](../../crates/kenvoy/frontend/package.json)、[前端锁文件](../../crates/kenvoy/frontend/package-lock.json)、[客户端构建适配](../../crates/kenvoy/src/client_build.rs) |
| JavaScript 执行 | [JavaScript 模块](../../crates/kenvoy/src/javascript/) |

## 维护与废弃、替代关系

- 本目录主题继续维护；本次按类型归档不表示任何主题已废弃。结构或职责变化时同步相关设计、契约和本清单，重大路线变化另建 ADR，并在[决策索引](../decisions/README.md)登记替代范围。
- [ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)替代 [ADR 0001](../decisions/0001-two-crate-architecture.md)的 Workspace 成员数量限制，不替代两个产品层的职责与依赖方向；不可把历史“两成员”描述当作当前工程清单。
- XML、业务化 KEngine、SSR/PostgreSQL 属于已退出的历史路线，追溯入口见[总体架构的历史基线](architecture.md#历史基线参考)。默认前端已采用原生 React 控件与 CSS；[前端设计](frontend-runtime.md)中迁移前 MUI 验证只保留历史含义，新增组件绑定以[当前契约](../contracts/component-bindings.md)及其原始证据为准。
- 历史 ADR、变更和验证正文不可变，不因目录整理合并、删除或重编号；后续变化另建记录并维护索引关联。现行接口不是后续计划，计划也不是已完成验收；本入口不新增测试结论。
