# 0025：dev/run 优雅退出与安全收尾

状态：已执行（2026-11-03）；治理：不可变变更记录。影响范围：Kenvoy CLI 的 `dev` / `run` 生命周期、Vite 开发子进程、SQLite/JavaScript 收尾、版本 0.2.1。范围不含 main 合并、远端发布或渠道上线。

## 背景

既有实现已在 Ctrl+C 后停止开发协调器、关闭开发 WebSocket、等待发布任务并关闭 JavaScript 与 SQLite，但仍有以下收尾缺口：

- 只监听 Ctrl+C，Unix 服务管理器常用的 SIGTERM 不会进入优雅退出；Windows 控制台关闭/注销事件也未处理。
- 信号监听在服务开始监听后才建立，`dev`/`run` 的启动构建阶段收到终止信号会直接结束，构建子进程和临时资源没有被有序回收。
- HTTP 优雅退出、JavaScript worker 回收和开发协调器等待没有统一期限；异常连接或线程可使关闭阶段长期挂起。
- Vite 关闭控制、子进程等待与日志管道读取没有完整期限；重复 Ctrl+C 也不能强制结束进程。
- 开发实例独占创建的 `.kenvoy/client-dev/<pid>-<时间戳>` 在正常退出后保留镜像与 Vite 缓存，形成无界运行残留。

## 方案

- 新增 `bootstrap::signals::Shutdown`，在 `dev`/`run` 的启动构建前安装：
  - Unix 同时处理 SIGINT 与 SIGTERM；Windows 处理 Ctrl+C、Ctrl+Break、控制台关闭与控制台注销，注册失败时退回 Ctrl+C。
  - 首次信号通过 `watch` 通知启动构建或 HTTP 宿主；从首次信号起最多等待 75 秒，超过期限强制退出。
  - 第二次信号立即以退出码 130 结束，并明确记录未完成写入需要核对。
  - `Shutdown` 持有监听任务，运行结束时中止；信号任务异常退出不会被误判成已收到退出请求。
- `dev`/`run` 在启动构建时与 `Shutdown::wait()` 竞争；构建阶段收到信号会取消构建并正常返回，依赖 `kill_on_drop` 终止本次子进程。
- HTTP 停止接收后最多等待 10 秒排空连接；开发 WebSocket 仍先取消。排空超时记录告警并继续关闭宿主资源，不无限等待。
- 开发协调器最多等待 15 秒；超时中止任务并报告可能存在遗留 Vite 进程。
- JavaScript worker 与 SQLite 线程由宿主分别以 15 秒期限回收；超时继续关闭其余资源并返回明确错误。
- Vite 关闭控制 3 秒、子进程等待 4 秒、强制终止与回收各 2 秒、日志管道 2 秒；日志管道的后台任务在超时后中止。
- Vite 正常关闭或控制超时后，删除本次实例独占的 `.kenvoy/client-dev/<pid>-<时间戳>`；父目录仅在为空时删除。
- Kenvoy 版本升至 0.2.1；KEngine 代码未变，保持 0.2.0，不发布无变更的 KEngine 新版本。

## 实施文件

- `crates/kenvoy/src/bootstrap/signals.rs`：信号安装、二次信号/超期强制退出、共享等待。
- `crates/kenvoy/src/bootstrap/commands.rs`：启动构建可取消、`Shutdown` 传入运行宿主。
- `crates/kenvoy/src/bootstrap/server.rs`：宿主关闭阶段期限与错误。
- `crates/kenvoy/src/bootstrap/error.rs`：`ShutdownTimeout` 诊断。
- `crates/kenvoy/src/bootstrap/lifecycle.rs`：开发协调器关闭期限。
- `crates/kenvoy/src/interfaces/http/runtime.rs`：HTTP 排空期限。
- `crates/kenvoy/src/infrastructure/client_build/development/{mod.rs,protocol.rs}`：Vite 控制/回收/日志期限与临时目录清理。
- `crates/kenvoy/src/infrastructure/client_build/tests/custom.rs`：正常关闭后开发目录为空的断言。
- `crates/kenvoy/Cargo.toml`、`Cargo.lock`、`crates/kenvoy/README.md`：0.2.1 版本同步。
- `xtask/src/commands/distribution/archive.rs`、`xtask/src/commands/version_tags/manifest.rs`：修复两条既有严格 Clippy 告警，不改变行为。

## 执行验证记录

环境：Windows 10.0.26300 x64、Rust 1.99.1、Node 24.22.0、最终 Kenvoy 0.2.1 二进制；构建使用 `--offline --locked`。

| 命令/场景 | 实际结果 |
|---|---|
| `cargo check -p kenvoy --bin kenvoy --offline --locked` | 退出 0 |
| `cargo test -p kenvoy --bin kenvoy interfaces::http::tests::publication::real_loopback_http_serves_metadata_and_stops_gracefully --offline --locked` | 1 项通过，退出 0 |
| `cargo test -p kenvoy --bin kenvoy custom_build --offline --locked -- --ignored --nocapture` | 1 项通过，退出 0；新增断言确认关闭后 `.kenvoy/client-dev` 无残留条目 |
| `cargo test --workspace --offline --locked` | 最终重跑通过，退出 0 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 修复上述两条告警后通过，退出 0 |
| 基础 `dev`：最终 0.2.1 二进制、独立临时项目、端口 38137、PTY 发送 Ctrl+C | 日志记录首次信号（75 秒期限）与“HTTP、开发任务、JavaScript 与数据库已关闭”；端口关闭；无本项目 kenvoy/node 残留；无 `client-dev` 目录 |
| 自定义组件 `dev`：最终 0.2.1 二进制、临时项目、端口 38138、启动前确认 Vite node 子进程存在 | Ctrl+C 后日志显示完整关闭；端口关闭；`node.exe` 计数 0；`client-dev` 目录不存在 |
| 生产 `run`：最终 0.2.1 二进制、同一基础项目、端口 38137、PTY 发送 Ctrl+C | 日志显示完整关闭；端口关闭；无本项目 kenvoy 残留 |

首次并行全量测试中，`kengine` 的 `cycles_and_atomic_file_replacement_update_with_no_cache_leak` 因测试临时目录同名返回 `AlreadyExists`；该用例单独重跑通过，随后全量重跑通过，未修改该测试夹具。此现象作为既有测试竞态保留，不计入本变更的关闭逻辑结论。

PTY 服务层对 Ctrl+C 报告外层 `exit=512`，该值不是应用进程自身的退出码；本记录只依据应用关闭日志、端口、子进程与临时目录观测，不把它写成应用退出 0 的成功证据。

## 已知限制

- Unix SIGTERM 分支和 Windows 控制台关闭/注销分支本次仅在 Windows 上编译；实际信号路径只执行了 Ctrl+C。
- HTTP 排空超时、JavaScript/SQLite 关闭超时、开发协调器强制中止、第二次信号和 75 秒强制退出未在本次故障注入中触发。
- 启动构建取消依赖 Future 取消与 `kill_on_drop`；未执行构建中途 Ctrl+C 的真实场景。
- 强制退出时已接受但没有响应结果的写入仍需按既有规则核对；本变更不把未知结果宣称为未提交或已提交。
- 版本 0.2.1 仅在 dev 本地提交，未合并 main、未推送、未发布；KEngine 仍为 0.2.0。
