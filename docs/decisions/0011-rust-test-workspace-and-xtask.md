# ADR 0011：Rust 测试成员与 xtask 工程入口

状态：现行。日期：2026-11-02。替代 [ADR 0001](0001-two-crate-architecture.md) 的 Workspace 只能有两个成员限制，保留两个产品层及其单向依赖。

## 背景与问题

用户明确要求移除 tools 目录，以 Rust 标准测试承载测试脚本，新增专用跨 Crate 测试成员，同时保留每个 Crate 的独立 tests；必要的构建等工程编排可放根目录 xtask。原 tools 中混有 Node 工程检查、真实 HTTP 脚本、浏览器进程夹具及早期前端探针，执行入口分散。

## 可选方案

1. 继续把所有测试塞入 Kenvoy，并只把脚本挪目录：不满足专用测试成员与 Rust 所有权要求。
2. 为测试暴露 Kenvoy 内部 Library：增加产品公共契约并破坏唯一产品 Binary 的边界。
3. 保持两个产品 Crate，新增不发布的测试成员与 xtask：通过公共 KEngine API、真实 CLI 和标准 Cargo 测试形成跨层覆盖。

## 决策与原因

选择方案 3。Workspace 明确四个成员：kengine、kenvoy、kenvoy-tests、xtask；默认仍为两个产品包。产品不依赖内部工程成员。跨 Crate 业务断言和 CLI 子进程生命周期由 Rust 管理，架构/文档检查和构建入口由 Rust xtask 管理。

React 源码测试继续使用前端原生工具。浏览器仍使用已有 Playwright API，但由 Rust 标准测试发现与运行，JavaScript 仅做浏览器适配及 DOM 断言；不重复实现浏览器协议或另行安装新浏览器工具。

## 已知代价

跨进程测试需要先构建 CLI，xtask 从 Cargo 编译消息解析准确二进制路径。显式浏览器用例需要已准备的 Node/npm 和 Chromium，并保留临时项目锁定依赖的安装验证。Rust 与浏览器适配间的消息协议需要超时、错误传播及失败清理测试。xtask 只实现当前实际需要的任务，不预建代码生成或发布流程。

## 对既有架构的影响

KEngine 的通用性、Generic IR 与 Application IR 的分离，以及 Kenvoy 的业务运行职责保持原有边界。各产品 Crate tests 保留；旧独立前端探针由正式应用的浏览器测试接替，原 tools 路径退役。历史 ADR、验证与变更正文保留，通过索引和新记录说明替代关系。
