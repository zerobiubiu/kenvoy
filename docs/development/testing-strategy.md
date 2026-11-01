# 测试与验收策略

状态：参考（现状文档）；按实际行为分层，不以模块数量、空测试或探针替代产品验收。

| 层级 | 实际位置 | 核心证据 |
|---|---|---|
| 语言 | crates/kengine/tests | TSX、表达式、词法作用域、模块链接、位置与增量 |
| DSL/验证 | kenvoy/src/application/compilation、domain/validation、tests/dsl_security.rs | 静态结构、业务冲突、边界和共享标量向量 |
| SQLite | kenvoy/src/infrastructure/persistence/sqlite 下的分模块测试 | 临时文件、事务、迁移、并发、关闭与重开 |
| 认证/HTTP | application/identity/tests.rs、interfaces/http/tests | 哈希/会话、权限、CSRF、限流、静态隔离、真实 TCP |
| CLI 独立测试 | kenvoy/tests | 真实 Binary、受控 stdin、配置与迁移 |
| 跨 Crate | kenvoy-tests/tests | KEngine 公共分析与 Kenvoy CLI、真实 HTTP/认证及持久化 |
| 前端 | kenvoy/frontend/tests | 与 Rust 共用 68 向量、API 状态和失败语义 |
| 浏览器 | kenvoy-tests/tests/browser.rs + kenvoy/frontend/scripts | Rust 管理 CLI/临时项目/进程，Playwright 执行真实 DOM 断言 |
| 结构门禁 | xtask/src/checks/architecture | 真实 Cargo 图、AST 依赖/别名/门面/测试 cfg、前端注册表隔离与文件规模 |

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
cargo xtask check
cargo xtask test
cargo xtask browser
```

依赖已准备时可加 --offline。xtask 内部 Cargo 命令使用 offline/locked；test 先构建正式 CLI 并提供 KENVOY_BINARY 路径，避免误用旧二进制。KEngine 单独 check/test 不需要前端或数据库。npm 安装不下载 Playwright 浏览器；真实浏览器验证还需已准备的兼容 Chromium。当前使用的 Chromium 版本以各报告为准。完整接口见[工程与测试流程](engineering-workflow.md)。

测试只创建唯一临时项目及数据库，清理限于本测试拥有的目录和进程。网络测试绑定 loopback/系统空闲端口。API mock 只测试客户端错误处理，不能证明真实数据闭环；浏览器验收必须连接真实 Kenvoy。Windows 子进程 SIGTERM 是强制终止，不能将其称为 Ctrl+C 优雅退出。

所有阶段报告记录执行环境、命令、退出码、失败与修复、未覆盖项目。历史 `p01..p05` 是技术探针；其中依赖已退役 tools 前端的浏览器用例移除，由测试成员中的七套正式应用流程接替。其余 Rust 独立测试保留。显式忽略的跨工具测试必须单独调用，不能计入普通 cargo test 的通过数量。严格 Clippy 的剩余告警不能用 allow 或公开无意义接口掩盖。

跨层重构另保留旧 CLI 基线，逐字比较示例 `inspect` 与 `--help`；用重构前真实程序创建独立 SQLite/会话/附件，再由新程序读取并比较记录与文件字节。结构门禁不能替代运行回归；本轮实际证据见 [0016](../changes/0016-ddd-structure-and-module-split.md)，不据此扩大平台或真实 Ctrl+C 验收范围。

最终 E2E 已覆盖完整后台、自定义 UI/Server Function、错误恢复、热更新、生产自动构建与 artifact-only 部署；专项终端测试另验真实 Ctrl+C。阶段 10 的命令、计数与平台边界见[首版验收](../verification/0012-final-acceptance.md)，本次 Rust 工程入口的实际验证见[0013 变更记录](../changes/0013-rust-test-workspace-and-tools-retirement.md)，不以阶段 06 的初次表单证据替代最终结果。
