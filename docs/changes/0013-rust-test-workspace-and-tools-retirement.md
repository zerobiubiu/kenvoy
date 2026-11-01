# Rust 测试成员、xtask 与 tools 退役

状态：已执行。日期：2026-11-02。前置：[0012](0012-native-components-and-options.md)；决策：[ADR 0011](../adr/0011-rust-test-workspace-and-xtask.md)。

## 背景与方案

用户明确要求删除 tools 目录，将测试脚本改为 Rust 标准测试，新增专门跨 Crate 测试成员并保留各产品 Crate 的独立 tests；工程编排允许放根目录 xtask。该指令替代最初 Workspace 只能有两个成员的数量限制，产品职责和依赖方向继续保持。

方案为两个产品成员、一个不发布的测试成员、一个不发布的工程成员。真实 CLI/HTTP/SQLite 场景和临时项目/进程所有权迁入 Rust；浏览器操作保留已有 Playwright，通过 Rust 标准测试发现与运行。前端源码单元测试保留原生工具，不重复实现浏览器协议。

## 实施范围

| 范围 | 文件与变化 |
|---|---|
| Workspace | Cargo.toml 新增 kenvoy-tests/xtask，默认仍两个产品；Cargo.lock 同步；.cargo/config.toml 新增 xtask 别名 |
| 工程检查 | xtask/ 新增 Rust 架构、文档检查和构建/测试编排 |
| 集成测试 | crates/kenvoy-tests/ 新增标准测试、临时项目、进程守卫、跨语言/宿主场景及 HTTP 持久化验证 |
| 浏览器 | 测试成员中的 browser.rs 管理七套正式应用流程；薄 JavaScript 协议桥替代 Node 进程夹具；原前端 DOM 断言保留 |
| 退役 | tools 全目录移出项目树；p03_http_sqlite.rs 仅删除依赖旧独立前端的浏览器用例与专属 helper，其余 Rust 测试保留 |
| 文档 | AGENTS、README、架构/边界/测试/技术栈/索引更新；新增工程流程、本记录及 ADR 0011；历史正文保留 |

递归删除 D:\kenvoy\tools 的命令在执行前被自动审批拒绝，返回 blocked by policy，没有产生删除副作用。随后选择更安全的可恢复归档：将已核对的 tools 目录移至忽略的 artifacts/retired-tools-20261101T154914；原 tools 路径已不存在，归档不进入 Cargo、Git 或现行运行依赖。旧源码仍可从 Git 1ccc2db 追溯。

没有迁移用户数据库，没有新增产品 Library 或开放内部业务 API。没有调整产品版本、推送、标签或发布。

依赖只复用当前 Cargo.lock 的包版本。Windows 测试成员显式依赖已有 windows-sys 0.61.2，启用 Foundation、Security、JobObjects、Threading，通过自有 Job 管理子进程树；父进程提前退出后仍等待后代退出。Unix 使用独立进程组，但本轮只实测 Windows。临时目录清理有界重试并报告失败路径。

浏览器协议使用有界事件和响应队列、1 MiB 单行限制、写入确认期限与整体期限。故障回归覆盖非法消息、提前退出，以及对端不读取 8 MiB 响应时必须终止并回收写入线程，不让阻塞管道绕过总超时。

## 验证边界与后续建议

测试成员使用已构建的正式 CLI，不在 HTTP 请求或测试过程中隐式执行 Cargo 构建。xtask 从 Cargo 编译消息选取二进制并传递路径。浏览器测试为显式 ignored 用例，环境需已有 Playwright/Chromium；仅临时扩展/选项项目保留声明依赖的显式安装验证。

Windows 下进程强制终止不等于真实 Ctrl+C。跨平台执行、长时压力及断电恢复仍按既有风险清单跟进，历史验收不冒充本次重组后的结果。

## 执行验证记录

环境：Windows/PowerShell；Rust/Cargo 1.99.1，最低版本编译检查 Rust 1.98.0；Node 24.22.0、npm 12.2.0、Chromium 156.0.8078.4。Rust 命令使用当前锁文件及离线缓存；没有安装新的浏览器或工具。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo xtask build` | 退出 0；真实前端构建后，通过 Cargo compiler-artifact 定位当前 CLI |
| `cargo xtask test` | 退出 0；225 项标准 Rust 测试通过，8 项显式跨工具用例默认忽略；子进程看门狗的嵌套输出不重复计数 |
| `cargo test -p kenvoy-tests --offline --locked` | 清理细节最终收紧后退出 0；9 项普通测试通过，7 项浏览器用例忽略；其中 HTTP 用例覆盖原 20 次真实请求断言 |
| `cargo xtask browser -- --nocapture` | 退出 0；7 套正式应用浏览器流程通过，49.79 秒；包括静态/远程选项、系统重载、后台、HMR、监听恢复、生产和仅产物部署 |
| `cargo test -p kenvoy --offline --locked -- --ignored` | 退出 0；剩余 1 项真实用户组件安装/构建缓存测试通过，9.35 秒 |
| `cargo check --workspace --all-targets --offline --locked` | 退出 0 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 退出 0；无放宽告警规则 |
| `cargo +1.98.0 check --workspace --all-targets --offline --locked` | 退出 0；四成员全部目标在最低版本可编译，不等同全部运行测试 |
| `cargo fmt --all -- --check` | 最终退出 0 |
| `cargo xtask check` | 退出 0；四成员、默认成员、禁止发布与传递依赖边界通过；文档编码、链接和索引可达性通过 |
| frontend：`npm run check`、`npm test` | 均退出 0；101 项测试通过 |
| 浏览器薄桥与 7 个脚本的 `node --check` | 全部退出 0 |
| `git diff --check`、原 tools 路径检查 | 退出 0；D:\kenvoy\tools 不存在，归档受 artifacts 忽略规则保护 |

新成员最初编译前需要将内部包写入锁文件，浏览器测试也曾提示缺少 serde 声明；补齐已有版本的依赖并更新锁文件后通过。文档校验准确发现 changes 索引仍链接已退役 tools，修正索引后通过。初次格式检查发现旧探针移除后的 import 与新增回归排版差异，格式化后通过。

只读复核发现浏览器桥的同步管道写入及无界事件通道可能绕过期限或积累内存；修为有界通信并新增真实故障回归后，普通测试及七套浏览器流程均通过，未删除业务断言。进程回收额外验证了父进程先退出、后代仍监听时，自有 Job/进程组必须关闭其监听端口。

部分 Rust 命令仍输出既有 Windows incremental 缓存目录拒绝访问的 note，检查和测试最终退出 0；没有修改权限或删除缓存规避。全量、浏览器、最低版本和显式测试日志保存在忽略的 artifacts/rust-test-workspace-*.log。尚未执行 macOS/Linux 同等运行矩阵，也未把本轮强制终止声称为 Ctrl+C 优雅退出。
