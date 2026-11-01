# 测试与验收策略

状态：现行；按实际行为分层，不以模块数量、空测试或探针替代产品验收。

| 层级 | 实际位置 | 核心证据 |
|---|---|---|
| 语言 | crates/kengine/tests | TSX、表达式、词法作用域、模块链接、位置与增量 |
| DSL/验证 | kenvoy/src/dsl、validation.rs、tests/dsl_security.rs | 静态结构、业务冲突、边界和共享标量向量 |
| SQLite | kenvoy/src/database 的测试 | 临时文件、事务、迁移、并发、关闭与重开 |
| 认证/HTTP | auth_tests.rs、http/tests.rs | 哈希/会话、权限、CSRF、限流、静态隔离、真实 TCP |
| CLI | kenvoy/tests、tools/tests | 真实 Binary、受控 stdin、跨进程迁移与持久化 |
| 前端 | kenvoy/frontend/tests | 与 Rust 共用 68 向量、API 状态和失败语义 |
| 浏览器 | kenvoy/frontend/scripts | Playwright 真实表单、真实 API/SQLite、桌面和移动端 |

完整 Rust Workspace 验证前先构建框架内置前端：

```text
npm ci --ignore-scripts --prefix crates/kenvoy/frontend
npm run check --prefix crates/kenvoy/frontend
npm test --prefix crates/kenvoy/frontend
npm run build --prefix crates/kenvoy/frontend
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo fmt --all -- --check
node tools/verify-architecture.mjs
node tools/verify-docs.mjs
```

依赖已准备时可加 --offline。KEngine 单独 check/test 不需要前端或数据库。npm 安装不下载 Playwright 浏览器；真实浏览器验证还需已准备的兼容 Chromium。当前使用的 Chromium 版本以各报告为准。

测试只创建唯一临时项目及数据库，清理限于本测试拥有的目录和进程。网络测试绑定 loopback/系统空闲端口。API mock 只测试客户端错误处理，不能证明真实数据闭环；浏览器验收必须连接真实 Kenvoy。Windows 子进程 SIGTERM 是强制终止，不能将其称为 Ctrl+C 优雅退出。

所有阶段报告记录执行环境、命令、退出码、失败与修复、未覆盖项目。历史 `p01..p05` 是技术探针；显式忽略的跨工具测试必须单独调用，不能计入普通 cargo test 的通过数量。严格 Clippy 的剩余告警不能用 allow 或公开无意义接口掩盖。

最终 E2E 已覆盖完整后台、自定义 UI/Server Function、错误恢复、热更新、生产自动构建与 artifact-only 部署；专项终端测试另验真实 Ctrl+C。最新命令、计数与平台边界见[最终验收](verification/0012-final-acceptance.md)，不以阶段 06 的初次表单证据替代最终结果。
