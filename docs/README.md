# docs 文档索引

| 文档 | 类型 | 状态 | 说明 |
| :--- | :--- | :--- | :--- |
| [01-ir-contract.md](./01-ir-contract.md) | 📋 接口契约 | ✅ 已执行 | kengine 与渲染器之间的 IR 中间表示契约（v0.1） |
| [02-demo-plan.md](./02-demo-plan.md) | 🔧 开发指导 | ✅ 已执行 | demo 最小闭环实现清单（XML → IR → HTML） |
| [03-project-structure.md](./03-project-structure.md) | 📄 参考说明 | 📄 参考 | 目标项目结构（源自设计会话） |
| [04-storage.md](./04-storage.md) | 📋 接口契约 | ✅ 已执行 | 存储架构：PostgreSQL 18+ + S3（元数据 + 上传/下载端点） |
| [history.md](./history.md) | 🧾 变更记录 | 📄 参考 | 项目变更历史（追加型，只增不改） |

## 推荐阅读顺序

1. `01-ir-contract.md` — 先理解 kengine 对外产物的形态
2. `02-demo-plan.md` — 再按清单理解最小闭环的实现范围

## 代码位置

- IR 类型：`crates/kengine/src/ir/`
- XML 解析：`crates/kengine/src/parse/`（表达式 `expr` / 校验 `validation` / 状态机 `workflow`）
- IR→HTML 渲染：`crates/kengine-html/`（Tabler/Bootstrap 5，运行时 `assets/runtime.js`）
- 数据层：`crates/kengine-db/`（PG 18 系统表 + 模型动态建表 + 数据鉴权）
- 对象存储：`crates/kengine-s3/`（S3 抽象，MinIO 联调通过）
- CLI：`crates/kenvoy/`（init / dev / build / deploy / user / role / contact / department）

## 已废弃 / 替代关系

无。
