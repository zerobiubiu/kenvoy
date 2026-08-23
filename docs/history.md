# 变更历史

> 追加型历史文档：只增量追加，不重写已有历史。

## 2026-08-23 — 确立 IR 契约与 demo 最小闭环方案

- 需求摘要：为 Kenvoy 语义化低代码平台确立「XML → IR → HTML」渲染链路的设计与实现计划
- 执行结果：新增 `docs/01-ir-contract.md`（IR 契约 v0.1）与 `docs/02-demo-plan.md`（demo 实现清单）
- 影响范围：`kengine` 对外产物形态（IR JSON）、渲染器分层（`kengine-html`）、`kenvoy` CLI 的 `dev` 命令
- 验证情况：设计文档，尚未实现；B1（IR 类型）动工后在本条目下追加执行验证记录
- 关联文档：`docs/01-ir-contract.md`、`docs/02-demo-plan.md`

### 执行验证记录（2026-08-23，B1 完成）

- 环境：本地 Windows，Rust 1.98
- 命令：`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：`crates/kengine/src/ir/` 新增 `IR` 类型（`ProjectIr` / `Model` / `Layout` / `Page` / `Node` 及 props）+ serde round-trip 测试
- 决策确认：D1~D6 全部确认；`Custom` 组件因 serde 内部标签限制改用 `component` 字段承载组件名（契约已同步更新）
- 已知限制：B2（XML 解析）未开始；`flow` / `permissions` 为占位结构
