# 开发与工程文档

状态：参考；治理：现状文档。

返回[文档总索引](../README.md)。

## 目录职责

集中维护阶段状态、技术风险、测试策略与工程流程，回答“当前完成了什么、还有哪些边界、如何继续开发与取得证据”。本目录不是历史执行记录，也不把后续方向视为已排期、已授权或已验收的工作。

## 文档清单

| 文档 | 状态 | 内容概览 |
|---|---|---|
| [阶段状态与后续工作](development-roadmap.md) | 参考 | 阶段 00—10 的完成范围、首版后的进展、未完成工作分组与后续实施原则 |
| [后续实施与验证待办](implementation-backlog.md) | 参考 | 实现完善、验证补齐与可选扩展的当前边界、代码入口、后续工作和验收标准；排除已确认设计差异 |
| [技术风险与验证门禁](technical-risks.md) | 参考 | 语言、业务模型、数据、权限、构建与生命周期风险；区分已实施控制、已有证据和剩余边界 |
| [测试与验收策略](testing-strategy.md) | 参考 | 语言、DSL、数据库、HTTP、CLI、跨 Crate、前端与真实浏览器的分层覆盖及环境要求 |
| [Rust 工程入口与跨 Crate 测试](engineering-workflow.md) | 参考 | Workspace 成员职责、xtask 命令、Rust 夹具与浏览器驱动边界，以及旧 tools 的替代入口 |

## 推荐阅读顺序

1. 先读[阶段状态](development-roadmap.md)，再查[后续待办](implementation-backlog.md)，区分基础验收、后续已实现变化、实现缺口、验证缺口和未排期的可选扩展。
2. 按拟修改范围查[技术风险](technical-risks.md)，确认数据保护、职责隔离与尚未覆盖的条件。
3. 用[测试策略](testing-strategy.md)确定最低充分验证，再到[工程流程](engineering-workflow.md)查实际命令、前置依赖与测试所有权。
4. 接口与配置以[契约目录](../contracts/README.md)为入口，实现机制以[架构目录](../architecture/README.md)为入口；用户操作见[CLI 当前行为](../operations/cli-design.md)。

## 阶段验收与首版后证据

- 第一版阶段 00—10 的综合结论见[阶段 10 最终验收](../verification/0012-final-acceptance.md)，各阶段原始环境、命令、失败与限制从[验证记录索引](../verification/README.md)进入。
- 首版后的原生组件与联动选项见[变更 0012](../changes/0012-native-components-and-options.md)，Rust 测试成员、xtask 与 tools 退役见[变更 0013](../changes/0013-rust-test-workspace-and-tools-retirement.md)。这些记录中的专项验证与基础阶段验收分开，不回写为当时已存在的能力。
- 历史测试数量只描述对应修订与环境，不累加不同轮次，也不直接作为当前修订的测试计数；已实现、设计与待验证事项以主题正文及原始证据为准。

## 相关代码与配置

- 工程成员、默认成员、edition 与 MSRV：[根 Cargo.toml](../../Cargo.toml)；`cargo xtask` 别名：[.cargo/config.toml](../../.cargo/config.toml)。产品与工程成员边界见[Crate 职责](../architecture/crate-boundaries.md)和[ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)。
- 产品实现与独立测试：[KEngine 源码](../../crates/kengine/src/)、[KEngine tests](../../crates/kengine/tests/)、[Kenvoy 源码](../../crates/kenvoy/src/)、[Kenvoy tests](../../crates/kenvoy/tests/)。
- 跨 Crate 集成与 Rust 夹具：[kenvoy-tests](../../crates/kenvoy-tests/)；工程编排与检查：[xtask](../../xtask/src/)。
- 前端原生单元测试与浏览器驱动：[frontend/tests](../../crates/kenvoy/frontend/tests/)、[frontend/scripts](../../crates/kenvoy/frontend/scripts/)；浏览器进程与临时项目的所有权由 Rust 夹具承担。
- 开发与文档治理约束：[AGENTS.md](../../AGENTS.md)；从源码构建的环境准备：[项目 README](../../README.md)。

## 维护、废弃与替代关系

本目录主题均为可持续维护的现状参考，当前无废弃主题。实现、接口或工程入口变化时更新相应主题；待办状态与关闭标准统一维护在[后续实施与验证待办](implementation-backlog.md)，由[变更记录](../changes/README.md)承载当次实施与验证事实；重大路线变化另建[决策记录](../decisions/README.md)。

历史 ADR、变更和验证正文不可变，不合并、不删除、不重编号；后续变化或勘误通过新记录及索引说明关联。原 tools 已退役，其检查由 xtask、跨 Crate 场景与进程夹具由 kenvoy-tests、独立前端探针由正式应用浏览器流程接替。旧路径和命令仅保留为历史事实，不作为当前开发依赖或执行入口。
