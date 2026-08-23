# 变更历史

> 追加型历史文档：只增量追加，不重写已有历史。

## 2026-08-23 — 确立 IR 契约与 demo 最小闭环方案

- 需求摘要：为 Kenvoy 语义化低代码平台确立「XML → IR → HTML」渲染链路的设计与实现计划
- 执行结果：新增 `docs/01-ir-contract.md`（IR 契约 v0.1）与 `docs/02-demo-plan.md`（demo 实现清单）
- 影响范围：`kengine` 对外产物形态（IR JSON）、渲染器分层（`kengine-html`）、`kenvoy` CLI 的 `dev` 命令
- 验证情况：设计文档，尚未实现；B1（IR 类型）动工后在本条目下追加执行验证记录
- 关联文档：`docs/01-ir-contract.md`、`docs/02-demo-plan.md`
