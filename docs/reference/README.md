# 技术栈参考资料

状态：参考；治理：现状文档。

返回[文档总索引](../README.md)。

## 目录职责

汇集技术栈、直接依赖、feature、官方资料与可行性边界，帮助区分技术选型依据、正式实现、历史探针和仍待验证的条件。这里不替代 API、配置或操作契约，也不以资料中描述的上游能力推定 Kenvoy 已支持该能力。

## 文档清单

| 文档 | 状态 | 内容概览 |
|---|---|---|
| [称呼与术语](terminology.md) | 参考 | “kenvoy 项目”“Kenvoy CLI”“KEngine”“本仓库”的唯一含义、歧义改法与强制使用规则 |
| [技术栈与可行性调查](technology-stack.md) | 参考 | 环境与证据等级、锁定的 Cargo/npm 直接依赖、SWC 与两层 IR、React/原生 CSS、Vite/Rolldown、QuickJS、SQLite/HTTP 的实现边界及官方依据 |

## 推荐阅读顺序

1. 先读[技术栈资料](technology-stack.md)中的证据等级与环境，再阅读所关注技术的依赖和限制，不把阶段 00 探针当成正式功能验收。
2. 查版本、feature 或依赖来源时对照下方 manifest 与锁文件；本入口不重复维护另一份版本表。
3. 了解机制时进入[整体架构](../architecture/architecture.md)及[技术决策现状](../architecture/technical-decisions.md)，追溯选择依据时进入[决策记录](../decisions/README.md)。
4. 使用公开接口或配置时查[契约目录](../contracts/README.md)，实际启动、构建与迁移时查[操作目录](../operations/README.md)；继续开发前核对[技术风险](../development/technical-risks.md)。

## 相关代码与配置

- Rust 版本、共享依赖与 Workspace：[根 Cargo.toml](../../Cargo.toml)、[Cargo.lock](../../Cargo.lock)。成员依赖见 [KEngine manifest](../../crates/kengine/Cargo.toml)、[Kenvoy manifest](../../crates/kenvoy/Cargo.toml)、[测试成员 manifest](../../crates/kenvoy-tests/Cargo.toml)和 [xtask manifest](../../xtask/Cargo.toml)。测试和工程依赖不等于产品依赖。
- 前端依赖与构建配置：[package.json](../../crates/kenvoy/frontend/package.json)、[package-lock.json](../../crates/kenvoy/frontend/package-lock.json)、[vite.config.mjs](../../crates/kenvoy/frontend/vite.config.mjs)、[tsconfig.json](../../crates/kenvoy/frontend/tsconfig.json)。用户应用自己的组件库由其项目依赖管理，不属于默认 Runtime 的依赖承诺。
- npm CLI 安装器依赖：[工程声明](../../.github/scripts/channels/npm_runtime/package.json)、[锁文件](../../.github/scripts/channels/npm_runtime/package-lock.json)；只服务于单包下载安装，不属于应用前端依赖。
- 实现入口：[KEngine](../../crates/kengine/src/)、[Kenvoy](../../crates/kenvoy/src/)、[React Runtime](../../crates/kenvoy/frontend/src/)。相关机制见[KEngine 设计](../architecture/kengine-design.md)、[前端 Runtime](../architecture/frontend-runtime.md)、[JavaScript Runtime](../architecture/javascript-runtime.md)和[数据库设计](../architecture/database-design.md)。
- 工程与验证入口：[xtask](../../xtask/)、[kenvoy-tests](../../crates/kenvoy-tests/)及[工程流程](../development/engineering-workflow.md)；公开数据结构见[Generic IR](../contracts/generic-ir.md)与[Application IR](../contracts/application-ir.md)，运行设置见[项目配置](../contracts/project-configuration.md)。

## 证据使用边界

第一版阶段 00—10 的实际命令、输出、计数与平台范围见[验证记录索引](../verification/README.md)和[阶段 10 最终验收](../verification/0012-final-acceptance.md)。选型资料、编译通过、运行测试与发行可行性分别陈述，不相互代替。

首版后默认原生组件与联动选项见[变更 0012](../changes/0012-native-components-and-options.md)，Rust 工程重组与 tools 退役见[变更 0013](../changes/0013-rust-test-workspace-and-tools-retirement.md)。这些专项证据独立于基础阶段验收；历史测试数量既不跨轮次相加，也不直接作为当前修订的测试数量。已实现与待验证状态以主题正文、现有实现和对应原始记录为准。

## 维护、废弃与替代关系

本目录主题为可持续维护的现状参考，当前无废弃主题。依赖版本、feature、平台或技术路线变化时，依据实际 manifest、锁文件、实现与验证记录更新技术栈正文，并同步相关契约或架构文档；重大选择变化另建决策记录，不只修改选型表。

默认原生组件路线见[ADR 0010](../decisions/0010-native-components-and-data-bindings.md)，工程入口替代见[ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)。原 tools 与独立前端探针已退役，旧命令、MUI 验证和当时依赖仅保留为历史事实，不能视为当前默认技术栈或执行入口。定稿历史正文不合并、不删除、不重编号；后续变化或勘误另建记录，通过索引关联。
