# 阶段 07：管理后台与前台复验

状态：已执行；环境：Windows、Rust 1.99.1、Node 24.22.0、Chromium 156.0.8078.4；日期：2026-11-01。

## 覆盖范围

真实 CLI 创建独立临时项目，通过隐藏标准输入初始化管理员。浏览器先提交张三，再经管理登录执行详情、编辑、新增、搜索、等值筛选、排序、分页、删除取消及确认。分页数据也通过同一授权 HTTP API 写入 SQLite，不使用 mock。

编辑仅发送改动字段，测试新增李四时未触碰 phone/age 为 null，再修改 age 后 phone 仍为 null。停止并重新启动 dev 后，李四记录及持久化 Session 有效。390px 移动端没有整页横向溢出，表格在自身容器滚动；1440px 桌面端展示资源导航。

前台在同一最新内嵌构建上复验必填、100 字上限、整数、最小值、真实提交、匿名读取拒绝和管理员登录；浏览器 pageerror 均为空。

## 执行记录

| 命令 | 实际结果 | 退出码 |
|---|---|---|
| `npm run check --prefix crates/kenvoy/frontend` | UI 小修后严格 TypeScript 通过 | 0 |
| `npm run build --prefix crates/kenvoy/frontend` | 主包 589.88 kB、后台分块 14.73 kB；保留超过 500 kB 警告 | 0 |
| `cargo build -p kenvoy --locked --offline` | 08 依赖 feature 调整需要更新 lock，locked 正确阻止 | 1 |
| `cargo build -p kenvoy --offline` | 更新本地 lock 后编译并嵌入最新资产；保留生命周期待消费 dead_code 警告 | 0 |
| `node crates/kenvoy/frontend/scripts/admin-e2e.mjs` | CRUD、详情、搜索/筛选/排序/分页、null 保持、重启及移动端全部通过 | 0 |
| `node crates/kenvoy/frontend/scripts/forms-e2e.mjs` | 同一构建前台验证、SQLite 提交和登录通过 | 0 |

测试初次执行出现三项测试定位问题：详情文字同时命中隐藏列表与详情；分页默认值实际为 10 而脚本假定 20；未触碰必填字段文案随 null 保持规则变化。分别限定详情 dd、显式选择每页 20、断言字段 aria-invalid；没有关闭产品校验或略过失败流程。

截图输出 `artifacts/phase06/` 与 `artifacts/phase07/`，已通过 ignore 排除产物。桌面/移动截图经读取核查；独立视觉收尾及完整自定义组件 E2E 留待最终前端验收。

限制：Windows 夹具使用强制终止服务，验证持久化而非 Ctrl+C 优雅退出。没有将此记录解释为 run、HMR、服务端函数或全部阶段 10 通过。此次未重复完整 Rust 测试集；后续源码阶段统一执行。
