# 0001：严格双 Crate 架构

- 状态：参考；阶段 00 设计记录，边界由任务明确规定，依赖与实现待验收。
- 影响范围：Rust Workspace、公共 API、模块归属。

## 背景

KEngine 要具备独立的源代码分析价值，Kenvoy 则需要解释业务并托管完整应用。当前仓库已有两个最小 Crate，但尚无框架业务实现。

## 问题

如果按数据库、HTTP、前端等职责继续拆成 Workspace Crate，会违反两个 Crate 的产品约束；如果把所有语义放进解析引擎，又会使引擎与 Web 业务绑定。

## 可选方案

1. 单 Crate：组织简单，但难以验证通用引擎的独立性。
2. 多个专用 Crate：依赖边界显式，但违反本项目约束。
3. 两个 Crate，内部 Rust Module 分层：同时满足产品边界与组织需求。

## 最终选择

Workspace 只包含 `kengine` 和 `kenvoy`。`kengine` 只提供 Library；`kenvoy` 提供唯一用户 Binary，并通过路径依赖调用 `kengine`。依赖方向严格为 `kenvoy → kengine`。

KEngine 仅处理解析、通用 IR、模块图、符号、引用和诊断；Web、SQLite、React、认证、迁移和业务语义归 Kenvoy。内部适配层采用 Module，不新增第三个 Workspace Crate。外部 Cargo 依赖不属于本项目自有 Crate 的数量限制。

## 原因

通用源代码结构与应用业务模型可以分别演进；独立构建 KEngine 能直接检查其依赖边界。Cargo 支持统一 Workspace 成员和锁文件管理。[Cargo Workspace 文档](https://doc.rust-lang.org/cargo/reference/workspaces.html)

## 已知缺点

Kenvoy 内部模块数量会增长，必须通过可见性、接口和定向测试管理耦合，不能依靠更多 Crate 隔离。

## 影响

阶段 01 需要补齐单向路径依赖；后续验收检查实际 Workspace 成员、Target 类型及依赖树。阶段 00 不变更现有 Cargo 配置，不把设计图当成已经建立的依赖关系。

## 验证与状态

本轮只核查仓库骨架与 Cargo 配置。完整边界验收须在阶段 01 检查 `cargo metadata`、`cargo check -p kengine`、`cargo test -p kengine` 和依赖树；KEngine 不得出现 Web 业务依赖或 Binary Target。
