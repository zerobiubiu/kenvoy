# demo 最小闭环实现清单

> 文档类型：🔧 开发指导
> 状态：🔧 待执行
> 影响范围：`crates/kengine`、`crates/kengine-html`（待建）、`crates/kenvoy`
> 前置文档：[01-ir-contract.md](./01-ir-contract.md)
> 替代文档：无
> 生效版本：—

## 目标

打通「XML 业务语言 → kengine 解析 → IR → IR→HTML 渲染 → 浏览器可见页面」的最小闭环，作为 demo 演示，并为长期架构打下不返工的底座。

## 约束

- demo 不引入任何前端框架（Leptos / React 均不用于 demo）
- IR→HTML 是纯 Rust 无 I/O 函数，长期作为 SSR / 边缘渲染核心保留
- 客户端运行时（Leptos 或 React）决策推迟到 demo 之后，两条路都不返工

## 实现阶段

### B0 前置（约半天）

- `kengine` 依赖：`roxmltree`（解析）、`serde`、`serde_json`、`thiserror`
- `kenvoy` 依赖：`axum`、`notify`（文件监听）、`tokio`、`tower-http`（静态文件）
- 新建 `crates/kengine-html`（渲染器，见 D2）

### B1 IR 类型 + 测试（约 1 天）

- 按 [01-ir-contract.md](./01-ir-contract.md) 第 4 节实现全部类型
- `serde` round-trip 单元测试
- 完成标志：`cargo test` 通过，IR JSON 可序列化 / 反序列化

### B2 XML 解析子集（kengine，约 2~3 天）

- 解析 `<Layout>` / `<Page>` / `<Model>` 三个根元素
- 实现 layout 逐级匹配（契约第 5 节）+ 引用校验（`LayoutNotFound` / `ModelNotFound`）
- 数据绑定 `{{ }}` 原样保留进 IR（不解析）
- **golden 测试**：`tests/fixtures/*.xml` → 期望 `.json`，文件对比
- 完成标志：给定示例 XML → 合法 IR JSON

### B3 IR→HTML 渲染（kengine-html，约 2~3 天）

- `render_page(&ProjectIr, &Page) -> String`（纯函数，无 I/O）
- 实现：Layout 链嵌套 + Slot 替换；Form（按模型渲染字段）；Table（表头 + 空态）；StatCard；Row/Col/Section（简单 CSS 类）；HTML 转义防 XSS
- 完成标志：给定 IR → 可打开的 HTML 字符串

### B4 kv dev 服务器（约 1~2 天）

- `kenvoy` 加 `dev` 子命令
- 扫描 `apps/` → kengine 解析 → kengine-html 渲染 → axum 输出
- `public/` 静态资源走 `/`
- `notify` 监听 → 文件变更全量重渲染（demo 先整页刷新，见 D4）
- 完成标志：`cargo run -p kenvoy -- dev` 打开浏览器看到页面，改 XML 刷新可见变化

### B5 demo 示例与验收（约 1 天）

- 写 2 个业务 XML：**请假申请**（表单）+ **我的申请**（表格）+ 根 layout（导航 + Slot + footer）
- 数据模型放 `apps/请假管理/models.kenvoy.xml`（见 D1）

## 验收标准

1. `kv dev` 一条命令起服务
2. 浏览器能看到根 layout → 页面内容的完整嵌套渲染
3. 改 XML 保存 → 刷新页面 → 页面变化
4. `cargo test --workspace` 全绿

**预计总工期：约 7~10 天（单人）**

## 决策点

| # | 决策点 | 建议 | 状态 |
| :--- | :--- | :--- | :--- |
| D1 | 数据模型文件位置 | `apps/<应用名>/models.kenvoy.xml` | 待确认 |
| D2 | IR→HTML 位置 | 独立 crate `kengine-html`（渲染器与解析器分离，避免返工） | 待确认 |
| D3 | dev 服务器框架 | axum + notify + tokio（后期 Kenvoy-Runtime 可复用 axum 生态） | 待确认 |
| D4 | demo 热更新 | 整页刷新即可；SSE/WebSocket 推送后置 | 待确认 |
| D5 | 解析库 | roxmltree（树遍历 + 校验更顺手，性能后置再考虑 quick-xml） | 待确认 |

> 决策点与 [01-ir-contract.md](./01-ir-contract.md) 第 7 节交叉引用。
