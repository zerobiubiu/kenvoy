# 0004：使用统一 Application IR 驱动应用解释器

- 状态：参考；阶段 00 设计记录，DSL 和运行探针待验证。
- 影响范围：Kenvoy DSL、Application IR、Resource/Page/API/Admin Runtime。

## 背景

同一资源需要驱动数据库、API、前台表单和后台管理。为每个页面和资源生成一套业务源码容易使模型分叉。

## 问题

需要明确唯一业务事实来源，并区分声明解释、客户端组件执行和服务端函数执行。

## 可选方案

1. 为每个 Resource/Page 生成 Controller、Repository 与 React 业务组件。
2. 在 KEngine 内添加全栈语义。
3. Kenvoy 将 Generic IR 转为 Application IR，统一运行时解释执行。

## 最终选择

采用方案 3。Kenvoy 定义标签注册表、静态求值规则、资源命名、权限、路由冲突规则和 Application IR。统一 Resource Runtime 执行 CRUD；前台、后台、API 与 Schema Engine 读取同一份验证后的资源定义。

Generic IR 到 Application IR 的步骤必须显式诊断无法静态确定的 Schema。普通 UI 组件只进入客户端模块注册；Schema Component 静态展开；Server Function 通过受控注册引用执行。HTTP 请求使用已发布的有效快照，不重新解析 TSX。

前端使用 Application IR 的权限过滤投影，不把内部文件路径、服务端模块源码、凭据或全量授权信息直接输出。Rust 状态持有完整业务模型；浏览器得到的可操作提示不能替代服务端权限检查。

## 原因

业务规则有单一来源，减少前台、管理端和数据库之间的差异。React 允许从组件类型和属性动态创建元素，为统一前端解释器提供 API 基础；这只是设计依据，不是本项目运行证明。[React createElement](https://react.dev/reference/react/createElement)

## 已知缺点

通用解释器需要覆盖状态、校验和错误反馈；首次设计不能依赖用户任意 JavaScript 执行来推导 Schema。

## 影响

开发更新先构建候选模型、诊断和 Schema Diff，所有必要验证成功后才发布。错误候选保留诊断，旧有效模型继续服务；数据库迁移失败不得切换到不匹配的模型。

## 验证与状态

本轮没有 DSL Analyzer 或 Application IR 实现。阶段 03 验证语义、重复、冲突与模块分类；阶段 04 以后逐步验证同一模型真实驱动 SQLite、API 和页面，禁止用静态结果冒充运行闭环。
