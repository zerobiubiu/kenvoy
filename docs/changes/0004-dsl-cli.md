# Kenvoy DSL、字段验证与项目命令

状态：已执行；日期：2026-11-01；前置：[KEngine 语义](0003-kengine-semantics.md)。

## 背景与实施

将通用源代码结构转为宿主业务模型，同时保持 KEngine 不认识页面、资源、权限或数据库。静态求值和业务语义分离，先收集资源再验证页面引用，失败不返回可激活的 Application IR。

新增 application.rs、dsl/、validation.rs、project.rs、error.rs；main.rs 接通 create/check/inspect。新增 CLI/安全集成测试及共享字段向量；创建 examples/customer-app。新增 clap 4.6.7 derive、toml 0.9.8、chrono 0.4.45（仅 std）普通依赖，并将已锁 serde/serde_json/thiserror 用于宿主。来源为 crates.io，安装到已有 Cargo 缓存，锁文件可追踪；未改变系统工具或 Git 历史。

新增 Application IR v1、KV1101—1106 静态诊断、KV2xxx 语义/模块边界诊断、KV3006 相对资产诊断和 KV4xxx 字段错误。具体错误和限制以类型定义及测试为准；HTTP 仍未实现。同步 DSL、应用模型、配置/CLI 和阶段状态文档。

## 执行验证

Windows MSVC、Rust 1.99.1。workspace check/test/Clippy 严格检查与 fmt 全部退出 0；121 项常规测试通过，1 项浏览器探针未重跑。真实 Binary create/check/inspect、离线记录校验、失败不覆盖和安全边界均有证据。命令、问题修复和剩余边界见 [阶段 03 验证](../verification/0005-dsl-cli.md)。

后续使用真实临时 SQLite 验证迁移、漂移、回滚和重启保留，随后接通 HTTP 与浏览器；不以阶段 03 的 IR 检查替代实际应用验收。
