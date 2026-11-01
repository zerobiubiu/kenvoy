# 阶段 09：完整 CLI 与开发/生产生命周期验收

状态：已执行；阶段 09 基础验收通过；日期：2026-11-01；环境：Windows PowerShell、Rust 1.99.1、Node 24.22.0、npm 12.2.0、Chromium 156.0.8078.4。

## 实现范围

七个 CLI 命令均调用正式实现。build 封存 Application IR、受限服务端模块、客户端资产及依赖清单，不访问业务数据库。run 在有源码时检查新鲜度并自动构建；无源码时校验版本化产物并运行。dev 使用 notify、AnalysisSession、候选迁移/发布、同源 Vite HTTP/WebSocket 代理和浏览器开发状态协议。

所有数据库验证均在唯一临时项目内进行，未迁移用户业务数据库。双 Crate 边界保持不变，KEngine 不接入 Axum、SQLite、React 或应用类型。变更范围见[阶段 09 记录](../changes/0010-cli-lifecycle.md)，决策见 [ADR 0009](../adr/0009-artifacts-and-development-host.md)。

## 已执行的定向与浏览器验证

| 命令 | 实际结果 |
|---|---|
| `npm run check --prefix crates/kenvoy/frontend` | 退出 0，严格 TypeScript 检查 |
| `npm test --prefix crates/kenvoy/frontend` | 90 项通过，退出 0 |
| `npm run build --prefix crates/kenvoy/frontend` | 退出 0；主 JS 593934 字节、Admin 14736 字节；保留上游 500 kB chunk 警告 |
| `cargo test -p kenvoy --bin kenvoy artifact::tests --offline --locked` | 2 项通过，退出 0 |
| `cargo test -p kengine --offline --locked` | 66 项通过，退出 0；lib/doc-test 的 0 项不计作功能测试 |
| `cargo +1.98.0 check -p kengine --offline --locked` | 退出 0，34.08 秒 |
| `node crates/kenvoy/frontend/scripts/extensions-e2e.mjs` | 退出 0：Hooks/Props/内联常量捕获、MUI/clsx/CSS barrel、Schema Component、TS/ESM/Promise Action、真实提交、私有代码隔离 |
| 同一 extensions E2E 的 HMR 序列 | 初始点击后数量 2；改组件文案后仍为 2，继续点击变 4；CSS 边框 5px→7px 保留 4；错误 TSX 保留旧 UI 与 4，修复后新文案及 4 均保留 |
| `node crates/kenvoy/frontend/scripts/watcher-e2e.mjs` | 首轮退出 0：真实 notify、错误不发布、安全新增字段、删除阻断、清单与旧数据保留；后续增加配置恢复与详细诊断回归，结果见最终门禁 |
| `node crates/kenvoy/frontend/scripts/production-e2e.mjs` | 退出 0：无需预 build、源码变化重建、共享数据库；源码与工具缓存移出项目、PATH 无 Node 后正常运行，账号管理可用，开发接口 404 |
| `node tools/tests/http-cli.mjs` | 20 项真实断言通过，退出 0；该脚本本身不证明 Ctrl+C |

框架构建、类型检查与定向 Rust 命令未关闭错误检查。Rust 验证进程设置 `CARGO_INCREMENTAL=0`，避免先前 Windows 缓存文件锁提示；没有改系统或仓库全局配置。

## 真实 Ctrl+C

独立 PTY 中运行 `kenvoy --project <唯一临时项目> dev`，另一个进程执行 `node crates/kenvoy/frontend/scripts/shutdown-observer.mjs observe <origin>`。浏览器通过真实表单保存记录后，向终端发送 Ctrl+C，再重启同项目并执行 observer 的 verify 模式。

- 基础 DSL：观察到“HTTP、开发任务、JavaScript 与数据库已关闭”；浏览器确认端口断开；重启后台查询到原记录，observer 两种模式均退出 0。
- 自定义组件：当前文档原生 WebSocket 活跃数为 1；Ctrl+C 后活跃数归零且端口关闭；重启同 SQLite 记录保留，再次正常关闭。只读 WMI 查询未见该项目残留 Node。
- PowerShell PTY 外层因 Ctrl+C 返回 1，不能记为 CLI 自身普通成功退出 0，也不能把它解释为未执行清理。清理证据来自原生宿主关闭日志、网络状态与重启后的真实记录。
- 首轮 observer 把导航前旧 Playwright WebSocket 句柄计入，记录 2→1 后失败。改为注入当前文档的原生 WebSocket.readyState 观察，保留网络句柄数作为诊断；没有放宽关闭断言，复验通过。

两个退出夹具的清理被自动审批审查拒绝，返回“blocked by policy”，未提供更具体理由。已停止的目录保留于 `C:\Users\root\AppData\Local\Temp\kenvoy-custom-shutdown-a64cafe2681640a58e738927ccdce083` 和 `C:\Users\root\AppData\Local\Temp\kenvoy-exit-3d5ae599008e4383a019f1d26985399a`；没有绕过审查删除。自定义项目没有遗留 Node。

## 联调失败与修复

1. Windows CLI 嵌套大型 Future 在主线程触发栈溢出；对 build/start/run 的嵌套 Future 使用 Box::pin。随后真实 CLI 正常启动。
2. 优化依赖仍位于 Vite deps_temp 时，受控代理先 realpath 并提前 404。仅对受控 deps 路径交给 Vite 等待优化完成，保留普通文件边界；首模块并发依赖回归和浏览器通过。
3. SDK→Runtime→组件注册表→自定义组件→SDK 形成循环，HMR 重建 Context。把 Context/useRuntime 提取为独立 context.ts，既有公共导出保留；真实状态保持通过。
4. 开发 @id/raw 路径可以绕过镜像检查；客户端边界增加 mirrorRoot 与框架 src 限定，真实拒绝测试通过。
5. 构建输入指纹遗漏 MTS/CTS 与部分字体/图片；补齐支持集并加入逐文件变化测试。
6. 首次 Workspace 检查中，Windows 非 canonical 根路径使篡改诊断误分为 Boundary。verify 统一 canonicalize，定向测试退出 0；不把首次全量失败改写成通过。
7. 无效配置或需重启字段恢复原文后，旧 Error 未清除。增加 rejected 状态，仅当输入匹配活动候选才清理；提交后失败不伪装为恢复。统一启动与 watcher 的结构化诊断，保留代码、类别、位置和建议。

## 最终门禁与边界

最终 `cargo check --workspace --all-targets --offline --locked`、严格 Clippy、build 和 fmt check 均退出 0；常规测试 201 项通过、2 项默认 ignored，随后用 `cargo test --workspace --offline --locked -- --ignored` 显式执行这两项，均通过。看门狗子进程不重复计数。最终 Rust 1.98 Workspace 全目标 check 再次退出 0。

最终六套浏览器脚本及 20 项真实 HTTP 断言均退出 0。watcher 新增的无效 TOML/监听端口恢复、输入保留、代际不重复发布及终端诊断断言通过。镜像限额在分配前检查，实际读取也受剩余容量限制；最终 HMR 再次验证通过。末轮 Clippy 曾指出测试模块之后仍有生产函数，移动测试模块到文件末尾后严格检查通过，没有添加 allow。

最终二进制、全部命令与验收映射见[阶段 10](0012-final-acceptance.md)。当前证据不覆盖断电、磁盘满、网络文件系统监听、所有混合导出 Refresh 形态、构建中途 Ctrl+C 或其他操作系统。QuickJS 限额不代表 OS 沙箱；不可信项目拒绝执行。历史构建版本暂不自动回收。
