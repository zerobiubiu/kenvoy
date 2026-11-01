# 0007：Rust 原生 HTTP 与统一生命周期

- 状态：参考；阶段 00 设计记录，Axum 集成与服务生命周期探针待验证。
- 影响范围：Kenvoy CLI、HTTP Adapter、权限和服务状态。

## 背景

`kenvoy dev` 和 `kenvoy run` 都需要独立启动完整应用，不能要求用户另外启动业务后端。

## 问题

API、认证、静态资源与 CSR 页面必须共存，开发辅助工具不能成为另一个业务宿主或形成权限分叉。

## 可选方案

1. JavaScript HTTP 服务承担后端：违反 Rust + Axum 的默认路线。
2. 每个资源单独生成服务：复杂且不符合统一解释执行。
3. Axum + Tokio + Tower/tower-http，由 Kenvoy 管理统一运行时。

## 最终选择

采用方案 3。Axum 读取已初始化的应用快照和通用 Resource Runtime，注册 `/api/*`、`/admin/*` 及已声明用户页面；生产静态资源从构建清单加载。内部路由与保留路径统一做冲突检查。不存在的 API、非法资源和资产路径不得被 CSR fallback 掩盖。

Axum 官方设计与 Tokio、Hyper 和 Tower 服务生态配合，可复用中间件和静态资源服务，但不会自动实现项目的认证授权。[Axum 文档](https://docs.rs/axum/latest/axum/)、[tower-http 静态文件服务](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html)

管理端依赖服务端 Session 与安全 Cookie，写操作落实 CSRF 与来源检查；公开创建仅对明确允许的表单资源开放。请求体、分页、排序字段和资源名均有界验证，详细错误与敏感信息不直接暴露给客户端。

## 原因

业务能力集中在 Rust 原生进程，开发与生产共享模型、数据库路径和权限规则；前端工具只负责资源处理。

## 已知缺点

同步 SQLite、JavaScript 执行、文件监听与 HTTP 必须协调关闭，不能把阻塞操作直接放入异步 handler。自定义前端开发辅助进程需要独立健康检查和退出回收。

## 影响

启动先完成配置、模型和迁移检查再开放监听；失败退出保留错误上下文。`Ctrl+C` 停止接收新请求、排空有界工作并释放数据库、JavaScript 和监听器。更新失败保留最后一次有效应用状态。

## 验证与状态

本轮未增加 Axum/Tokio 依赖，也未启动 HTTP 服务。阶段 05 验证真实 CRUD、认证授权、CSRF、请求体限制、404/500 和异常恢复；阶段 09 验证 dev/run 同一数据库与业务语义、优雅退出及辅助进程回收。
