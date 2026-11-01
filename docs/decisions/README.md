# 架构决策索引

状态：参考（现状文档）。保存架构选择的背景、备选方案、原因与代价；当前实现以对应主题文档为准。返回[文档总索引](../README.md)。

## 从哪里开始

- 了解整体边界：先读[架构](../architecture/architecture.md)与[Crate 边界](../architecture/crate-boundaries.md)，再读 ADR 0001—0004；工程成员新增及 tools 退役见 [ADR 0011](0011-rust-test-workspace-and-xtask.md)。
- 了解开发与部署选择：先读[运行时设计](../architecture/runtime-design.md)，再读 [ADR 0009：产物与开发宿主](0009-artifacts-and-development-host.md)。组件与数据扩展见 [ADR 0010](0010-native-components-and-data-bindings.md)。
- 核对实际通过范围：首版基础验收读[阶段 09 生命周期](../verification/0011-cli-lifecycle.md)与[阶段 10 最终验收](../verification/0012-final-acceptance.md)；首版后组件与工程重组的专项证据见[验证索引](../verification/README.md#首版后的专项验证)，不将旧验收扩大为后续能力的验收。

## 决策清单

按编号追溯决策形成过程。0001—0008 形成于阶段 00，0009 形成于阶段 09；正文中的“计划”“待验证”和原始状态词保留当时的语境。下表单独维护当前选择的适用性：“现行”表示选择仍有效，不能单凭决策文档判断某项功能已验收；执行证据见[验证索引](../verification/README.md)。

| 编号 | 文档 | 当前选择状态 | 对应现状文档 |
|---|---|---|---|
| 0001 | [双 Crate 架构](0001-two-crate-architecture.md) | 两个产品层保留，Workspace 成员数量限制由 [0011](0011-rust-test-workspace-and-xtask.md) 替代 | [Crate 边界](../architecture/crate-boundaries.md) |
| 0002 | [通用 SWC 解析层](0002-kengine-generic-parser.md) | 现行 | [KEngine 设计](../architecture/kengine-design.md) |
| 0003 | [版本化 Generic IR](0003-generic-ir-design.md) | 现行 | [Generic IR](../contracts/generic-ir.md)、[KEngine API](../contracts/kengine-api.md) |
| 0004 | [应用解释器](0004-kenvoy-application-interpreter.md) | 现行 | [Application IR](../contracts/application-ir.md)、[解释器分工](../architecture/interpreter-design.md) |
| 0005 | [SQLite 数据库](0005-sqlite-database.md) | 现行 | [数据库设计](../architecture/database-design.md)、[迁移协议](../operations/migration-design.md) |
| 0006 | [React CSR Runtime](0006-react-csr-runtime.md) | React CSR 保留，默认 MUI 由 [0010](0010-native-components-and-data-bindings.md) 替代 | [前端 Runtime](../architecture/frontend-runtime.md) |
| 0007 | [Rust HTTP Runtime](0007-rust-http-runtime.md) | 现行 | [运行时设计](../architecture/runtime-design.md)、[安全契约](../architecture/security-design.md) |
| 0008 | [JavaScript 执行](0008-javascript-execution.md) | 现行 | [JavaScript Runtime](../architecture/javascript-runtime.md) |
| 0009 | [产物与开发宿主](0009-artifacts-and-development-host.md) | 现行 | [运行时设计](../architecture/runtime-design.md)、[CLI](../operations/cli-design.md) |
| 0010 | [原生组件与统一数据绑定](0010-native-components-and-data-bindings.md) | 现行；替代 0006 的默认 MUI 选择 | [组件与选项契约](../contracts/component-bindings.md) |
| 0011 | [Rust 测试成员与 xtask](0011-rust-test-workspace-and-xtask.md) | 现行；替代 0001 的成员数量限制 | [工程与测试流程](../development/engineering-workflow.md)、[Crate 边界](../architecture/crate-boundaries.md) |

## 关联代码与配置

实现位于 [KEngine](../../crates/kengine/) 与 [Kenvoy](../../crates/kenvoy/)，Workspace 约束见 [Cargo.toml](../../Cargo.toml)。精确依赖版本、来源与未覆盖风险分别见[技术栈](../reference/technology-stack.md)和[风险清单](../development/technical-risks.md)。

## 维护、废弃与替代关系

定稿正文不改写、不重排编号。后续路线调整或事实勘误新增记录，并在本索引标明前置、废弃或替代关系。实现进展更新对应现状文档与[验证索引](../verification/README.md)，不回填为历史决策当时已经通过。

ADR 0011 允许两个不发布的测试/工程成员并退役 tools，保留产品层隔离。ADR 0010 替代 ADR 0006 的默认 MUI 选择，保留 React CSR；ADR 0009 补充开发/生产宿主方案。历史官方链接的核对时间见记录；`latest` 和主分支页面不构成依赖锁定。
