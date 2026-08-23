# docs 文档索引

| 文档 | 类型 | 状态 | 说明 |
| :--- | :--- | :--- | :--- |
| [01-ir-contract.md](./01-ir-contract.md) | 📋 接口契约 | 🔧 待执行 | kengine 与渲染器之间的 IR 中间表示契约（v0.1） |
| [02-demo-plan.md](./02-demo-plan.md) | 🔧 开发指导 | 🔧 待执行 | demo 最小闭环实现清单（XML → IR → HTML） |
| [history.md](./history.md) | 🧾 变更记录 | 📄 参考 | 项目变更历史（追加型，只增不改） |

## 推荐阅读顺序

1. `01-ir-contract.md` — 先理解 kengine 对外产物的形态
2. `02-demo-plan.md` — 再按清单实现最小闭环

## 代码位置

- IR 类型：`crates/kengine/src/ir/`（待建）
- IR→HTML 渲染：`crates/kengine-html/`（待建）
- CLI：`crates/kenvoy/`

## 已废弃 / 替代关系

无。
