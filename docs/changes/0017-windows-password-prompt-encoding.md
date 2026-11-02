# 0017：Windows 密码提示乱码修复

状态：已执行；治理：不可变变更记录；日期：2026-11-02。影响范围：CLI 交互式账号初始化/追加用户；前置：[0016](0016-ddd-structure-and-module-split.md)；现行操作：[CLI 账号管理](../operations/cli-design.md#账号管理)。不修改产品版本、依赖或数据库格式。

## 背景与原因

用户运行 `kenvoy.exe --project examples/uploads-app admin init --username admin`，两次中文密码提示乱码，但最后的中文错误日志正常；错误为“密码须为 12 至 128 个 UTF-8 字节，不能全为空白或含控制字符”。用户提供的输出作为失败证据，没有在其应用数据库上重跑。

锁定的 rpassword 7.5.4 在 `src/lib.rs::prompt_password_with_config` 中，使用 OpenOptions 打开默认 `CONOUT$` 并直接写入 UTF-8 字节，没有经过 Windows Unicode 控制台输出；在非 UTF-8 输出代码页下会乱码。其隐藏输入则通过 ReadConsoleW 读取，属于不同路径。源码提示文本与错误日志并未损坏，不应修改源文件编码、强制更改用户代码页或关闭隐藏输入。

最后的校验错误与显示乱码分开处理：现有证据不能判断用户实际输入了什么，也不能据此推定密码读取丢失。本轮保留原限制，不索取或记录用户密码。

## 方案与文件清单

| 文件 | 修改 |
|---|---|
| `crates/kenvoy/src/interfaces/cli/password.rs` | 新增私有 prompt helper：标准错误锁定后写入并刷新提示，释放锁后调用 rpassword::read_password；两次提示共用此路径 |
| `docs/operations/cli-design.md` | 说明 Unicode 提示、无字符/星号回显、回车确认及不变的密码约束 |
| `docs/README.md`、`docs/changes/README.md`、`docs/verification/README.md` | 更新新记录与证据入口 |
| 本文 | 保存原因、实际验证和覆盖限制 |

Rust 的终端标准错误输出负责 Unicode 控制台写入；不新增 unsafe/Win32 产品依赖，不设置系统或控制台代码页，不增加提示字符串分配。隐藏读取、二次确认、12—128 UTF-8 字节限制、控制字符/全空白拒绝、显式 `--password-stdin`、重复 init 不覆盖账号均保持。

## 执行验证记录

### 环境与方法

Windows x64；使用本轮重新构建的 `target/debug/kenvoy.exe`。普通测试验证管道与持久化行为；另用一次性 Windows PowerShell/.NET 夹具创建独占原生控制台，直接读取真实屏幕缓冲区，并通过 WriteConsoleInputW 注入测试按键，而非用管道或 mock 代替隐藏输入。

原生夹具仅在自有控制台设置测试代码页，通过显式控制台标准句柄启动真实 CLI，等待两次完整中文提示和关闭回显后输入。每项检查密码不在屏幕出现、退出后的输入模式恢复，以及 CLI 未改变代码页。准备阶段的管道句柄及脚本路径问题已经修正，不计作产品用例通过。

所有项目/账号/数据库均位于独占 `artifacts/password-console-1f529424-native/` 夹具；未操作 `examples/uploads-app` 或任何用户数据。每个原生场景实际执行 `admin init --username smokeadmin`，不使用 `--password-stdin`。

### 命令与实际输出

| 命令 | 实际结果 |
|---|---|
| `cargo fmt --all -- --check` | 最初发现一处链式调用换行；修正后退出 0 |
| `cargo build -p kenvoy --offline --locked` | 退出 0，更新用户当前使用的 debug CLI |
| `cargo test -p kenvoy --test admin_cli --offline --locked` | 退出 0；2 passed，0 failed；覆盖显式管道模式、UTF-8/长度边界及不覆盖已有管理员 |
| `cargo clippy -p kenvoy --all-targets --offline --locked -- -D warnings` | 退出 0，不新增告警豁免 |
| `cargo run --package xtask --offline --locked -- check` | 退出 0；架构/文件规模通过，83 个 Markdown 与 846 个本地链接通过 |

原生控制台脚本最终退出 0，以下四项均完成全部断言；非零 CLI 退出是所测拒绝分支，不是脚本失败：

| 场景 | 控制台代码页 | CLI 退出码 | 实际观察 |
|---|---:|---:|---|
| 中文及 emoji 有效密码 | 936 | 0 | 两次中文提示准确，隐藏输入，显示“已创建账号：smokeadmin，角色：Admin” |
| ASCII 有效密码 | 65001 | 0 | 两次中文提示准确，隐藏输入，账号创建成功 |
| 二次密码不一致 | 936 | 1 | 准确显示“两次密码输入不一致”，不回显密码，恢复输入模式 |
| 短密码 | 936 | 1 | 准确显示原 12—128 UTF-8 字节约束，不回显密码，恢复输入模式 |

原生输出及断言保存在本机忽略文件 `artifacts/password-console-1f529424-results.json`；一次性脚本、自有测试账号库和控制台进程已清理，不属于源码或发行物。

### 结论、限制与后续建议

本机原生 Windows 控制台已确认提示编码修复、有效输入可创建账号、错误输入仍拒绝，以及不回显和模式恢复。未以两个管道测试冒充真实交互验证。LSP 服务启动后退出；本次未改公开签名，采用已读源码、编译与实际终端验证。

本轮不声称完成其他平台/终端矩阵或全量浏览器回归。以后升级密码输入库或改变终端 I/O，应再次验证非 UTF-8 代码页、Unicode 输入、不回显和异常退出后的模式恢复；仅检查重定向后的 UTF-8 文本不足以覆盖本问题。
