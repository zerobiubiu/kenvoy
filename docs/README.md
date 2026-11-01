# Kenvoy 文档中心

状态：参考；治理：现状文档；核对日期：2026-11-02。

这里是项目详细文档的唯一入口，负责文档索引、规范和维护要求。业务设计、接口契约、操作指导与历史记录按类型放入子目录；项目执行规则见 [AGENTS.md](../AGENTS.md)。

## 从这里开始

| 目的 | 推荐阅读顺序 |
|---|---|
| 了解整体设计 | [架构总览](architecture/architecture.md) → [Crate 边界](architecture/crate-boundaries.md) → [架构分类](architecture/README.md) |
| 创建并运行应用 | [项目说明](../README.md) → [CLI](operations/cli-design.md) → [项目配置](contracts/project-configuration.md) → [示例](../examples/README.md) |
| 编写声明与扩展 | [DSL 语法](contracts/dsl-specification.md) → [组件与选项契约](contracts/component-bindings.md) → [前端 Runtime](architecture/frontend-runtime.md) → [JavaScript Runtime](architecture/javascript-runtime.md) |
| 参与开发与验证 | [工程流程](development/engineering-workflow.md) → [测试策略](development/testing-strategy.md) → [验证证据](verification/README.md) |
| 核对完成范围与未完成项 | [阶段状态](development/development-roadmap.md) → [后续实施与验证待办](development/implementation-backlog.md) → [验证证据](verification/README.md) → [技术风险](development/technical-risks.md) |
| 追溯方案与实施 | [架构决策](decisions/README.md) → [变更记录](changes/README.md) → [验证记录](verification/README.md) |

## 分类索引与内容概览

各分类 README 列出本目录全部文档、状态、阅读顺序、关联实现及废弃/替代关系；由本入口进入分类后查询具体文档。

| 分类入口 | 内容概览 | 治理方式 |
|---|---|---|
| [architecture/](architecture/README.md) | 整体架构、产品边界、语言与业务解释、前后端运行时、数据库、安全及当前技术选择 | 现状文档，维护当前设计与实现边界 |
| [contracts/](contracts/README.md) | KEngine API、Generic/Application IR、DSL 语法、配置、组件绑定与选项协议 | 现状文档，维护有效接口契约 |
| [development/](development/README.md) | 工程流程、测试策略、阶段状态、[后续待办](development/implementation-backlog.md)与风险登记 | 现状文档，区分完成事实、实现待办和待补证据 |
| [operations/](operations/README.md) | CLI 使用、开发与生产运行、迁移确认及备份恢复操作 | 现状文档，维护操作条件与失败边界 |
| [reference/](reference/README.md) | 技术栈、依赖来源、锁定版本和平台实测范围 | 现状参考资料，不替代锁文件与验收记录 |
| [decisions/](decisions/README.md) | 架构选择的背景、备选方案、原因、代价及替代关系 | 决策正文定稿后不可变，当前适用性由索引维护 |
| [changes/](changes/README.md) | 各次实施的背景、方案、文件范围、接口变化与验证 | 已执行记录定稿后不可变，后续变化另建记录 |
| [verification/](verification/README.md) | 初始审查、准备计划、阶段验收及变更内附专项验证入口 | 保留各次环境、命令、实际结果和限制 |

产品与视觉约定由仓库现有的 [PRODUCT.md](../PRODUCT.md) 和 [DESIGN.md](../DESIGN.md) 维护，不在这里复制。

## 文档规范

### 存放、分类与命名

- `docs/` 根目录只保留文档中心 `README.md`；详细文档按类型归入子目录，不再平铺，也不保留重复入口或旧路径兼容副本。
- 类型由目录体现，不在文档元数据重复填写“类型”。目录只存放文档及必要文档资源，不放业务代码；有实际内容时才创建分类，不预建空目录。
- 每个分类必须有 `README.md`，包含完整清单、内容概览、状态、阅读顺序、相关代码/配置链接，以及废弃与替代关系。
- 持续维护的现状文档使用固定语义文件名。编号记录只在各自目录内递增，不建立 `docs/` 全局编号，不按阶段号重排既有记录。
- 历史记录保留原文件名与编号；准备计划 `verification/p01-dependency-plan.md` 和初始报告 `verification/verification-report.md` 单独索引，不伪装成后续执行记录。

### 状态、元数据与治理分类

- 新增或维护的现状文档使用“参考”状态，并明确描述当前有效事实；内容中区分已实现、设计、候选、待验证和明确未支持的范围。
- 状态词为“已执行 / 待执行 / 参考 / 已过期 / 已废弃”。“已执行”表示对应实施已经发生、记录定稿，不代表方案永久有效或全部环境通过验收。
- 编号文档头部按需要注明状态、影响范围、前置文档、替代文档和生效版本等元数据；不为统一形式向历史正文回填后来才知道的事实。
- 现状文档随系统演进维护；定稿决策、变更和验证档案保留历史事实。后续变化或勘误另建记录，并在分类索引说明适用性、废弃或替代关系。
- 历史正文中的原状态词、旧目录名、命令、测试计数和当时未实现项保留原语境；目录迁移只调整位置与相对链接，不改写原验证输出或结论。
- 设计解释原因，契约定义输入输出，操作指导说明步骤与失败处理，历史记录说明某次实施与验证。交叉内容使用链接，不维护多份会独立过期的完整清单。

## 文档维护要求

- 源码变更同步更新受影响现状文档，并在 `changes/` 记录背景、方案、实施文件清单、执行验证和后续验证建议；纯文档整理不递归新增变更记录。
- 执行记录必须包含实际环境、命令、输出、退出码、结论和已知限制；只记录真实执行，不把计划、编译或静态推断写成运行验收。
- 新增文档同步维护所属分类清单及本文档中心索引，确保可以从本入口发现；迁移时同步修正仓库入口、示例、规则文件及其他文档中的引用。
- 验证证据可以独立保存，也可以随变更记录保存，但统一从[验证分类](verification/README.md)进入。首版阶段验收与后续专项验证分开，不累加不同修订或环境的测试数量。
- 按主题或阶段定期梳理索引、关联关系和重复的现状说明；不可变档案不因整理而删改历史事实。

## 文档验证

现有入口为 `cargo xtask docs`；依赖已准备时，离线锁定运行同一任务：

```text
cargo run --package xtask --offline --locked -- docs
```

该任务检查 UTF-8、异常字符、行尾空白、本地链接及从本中心的可达性，不校验外部链接、章节锚点或技术论断，也不代替产品验收。目录调整还需核对：分类清单与磁盘文件双向一致、分类入口齐全、旧路径不再作为有效引用、目录内编号无重复，以及历史正文未因迁移改写事实。
