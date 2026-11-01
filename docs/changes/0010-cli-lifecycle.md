# CLI、生产产物与开发生命周期

状态：已执行；日期：2026-11-01；影响范围：Kenvoy build/dev/run、版本化产物、notify、Vite 镜像、HTTP/WebSocket 代理和 React 更新。阶段 09 最终 Workspace 门禁及自定义 HMR 连接的 CLI 退出验证仍在执行，本记录不提前宣称全部通过。

## 背景与方案

阶段 08 已能运行自定义组件和服务端函数，但尚未完成生产产物复用、无源码部署、文件监听和正式 React 状态保留。本次在原有 Application IR、数据库线程及 HTTP 同代发布上补齐统一生命周期，不增加 Rust Crate，也不把业务配置、HTTP 或前端能力放入 KEngine。

build 先完整分析、准备 JavaScript 包并构建客户端，再封存独立版本及校验清单；run 根据输入和框架指纹判断复用或重建，不要求事先 build。没有源码时可仅加载完整产物，数据库仍位于独立持久化目录。

dev 复用 KEngine AnalysisSession，notify 回调只唤醒容量 1 的协调队列。180ms 去抖后构建候选，校验源码/资产指纹，准备开发镜像，再进入数据库迁移与 HTTP 发布屏障。安全迁移、IR、generation、静态资产和 JavaScript 包同代切换。提交后才更新 Vite 镜像和活动磁盘清单；这两个收尾步骤失败时必须说明运行态已提交，不能误报数据库回滚。

基础 DSL 直接使用内嵌 React/MUI 资产。自定义客户端采用 Vite/React 插件适配，读取已验证镜像，由 Rust 私有控制管道提交更新，不监听原项目源码直接绕过候选检查。浏览器只访问 Kenvoy 公开 Origin，HTTP 和 HMR 通过固定回环目标代理。

## 实施文件

- 新增 `crates/kenvoy/src/lifecycle.rs`：notify、内容指纹、候选协调、配置重载限制、错误恢复及资源关闭。
- 新增 `crates/kenvoy/src/client_development.rs`、`frontend/scripts/client-dev.mjs`：受控源/资产镜像、Vite 子进程、私有控制协议、模块更新和停止回收。
- 新增 `crates/kenvoy/src/http/development.rs`，修改 `http/{mod.rs,handlers.rs,tests.rs}`：开发状态、固定目标 HTTP/WS 代理、发布接口、首导航等待屏障及真实回环测试。
- 完善 `crates/kenvoy/src/{artifact.rs,build.rs}`：OS 构建锁、staging、版本目录封存、活动清单激活、哈希/路径/容量校验和输入指纹；增加受支持源码及静态资产扩展名的失效回归。
- 修改 `crates/kenvoy/src/{main.rs,project.rs,error.rs}`：完整 build/dev/run 接线、无源码产物加载、资源清理和错误上下文；admin/inspect/migrate 可消费已加载的应用产物。
- 修改 `crates/kenvoy/src/{client_build.rs,client_build_tests.rs,frontend.rs}` 和 `frontend/scripts/client-builder.mjs`：正式开发工具链接入、镜像路径限制、真实子进程验证、Windows 资产校验路径统一及构建边界回归。
- 修改 `crates/kenvoy/build.rs`：框架指纹纳入 KEngine manifest、构建/开发脚本等实际输入，避免同版本 CLI 行为改变后继续复用旧产物。
- 修改 `crates/kenvoy/frontend/src/{App.tsx,Runtime.tsx,api.ts,types.ts,sdk.ts}`，新增 `src/context.ts`：开发轮询、错误保留、刷新/重载决策；将 Context 与组件注册表分离，消除 SDK 循环导入导致的 Fast Refresh 状态重建。
- 扩展 `crates/kenvoy/frontend/tests/{api.test.mjs,client-builder.test.mjs}`；新增 `scripts/{watcher-e2e.mjs,production-e2e.mjs,shutdown-observer.mjs}`，扩展 `scripts/extensions-e2e.mjs` 与 `tools/tests/project-harness.mjs`：真实监听、生产部署、HMR 状态、错误恢复和退出观察。
- 修改根及 Kenvoy Cargo manifest/lock：notify 提升为正式依赖，启用 Axum WebSocket，加入成熟 HTTP/WS 客户端依赖；没有新增自有 Crate。
- 同步运行时、CLI、项目配置、安全、前端与技术栈等现状文档，新增本记录及阶段 09 验证记录；阶段 08 历史记录保持不变。

本阶段没有清理业务数据库、自动删除旧产物、提交 Git 历史或变更包版本。文件分类按本阶段相对前一阶段的实施内容描述，不把工作区尚未提交误认为所有文件都在本阶段新建。

## 接口与行为

`kenvoy dev/build/run` 均支持显式 `--install-deps`。框架工具链使用锁定缓存，用户缺失依赖只按既有 package.json 声明安装；不静默修改声明，不加载用户 Vite 配置或运行其 npm 生命周期脚本。已有完整生产产物及基础 DSL 不要求 Node 运行时。

产物布局为 `dist/manifest.json` 与 `dist/versions/<build_id>/`，版本内包含 `application.ir`、`client/` 和 `modules/server.json`。manifest 包含格式、框架版本/指纹、输入指纹、依赖信息、逐文件大小及 SHA-256。先封存版本，再激活清单；失败不覆盖有效版本，不接管外来非空输出目录。旧版本不自动回收。

HttpRuntime 使用 `publish_prepared(application, assets, javascript)` 发布同代快照；`enable_development`、`set_development_status`、`replace_development_proxy` 管理开发状态。`GET /_kenvoy/development` 返回 revision、generation、ready/building/error、reload 和可选脱敏错误。首次轮询建立基线，后续刷新按代际与状态决定；生产返回 404。

首导航在 building 时最多等待 10 秒，不持有模型读锁；完成后重新匹配当前路由和权限，超时返回 503 与 Retry-After:1。旧界面在编译错误期间保留输入；Schema 删除或其他破坏性变化不能自动迁移。数据库、输出目录、host/port/public_origin 变更明确要求重启。

`/_kenvoy/vite/*` 代理只访问固定回环端口，内部随机密钥仅由 Rust 注入；HTTP 和 upgrade 均校验。禁止转发 Cookie、Authorization、CSRF，禁止上游重定向及返回 Set-Cookie。HMR 要求同源 Origin、vite-hmr 协议，并限制连接、频率、帧、缓冲及发送/关闭期限。开发镜像之外的原项目资产和框架构建脚本不能借 @id/raw 路径进入浏览器。React preamble 使用独立同源模块，未放宽 inline script CSP。

Ctrl+C 先停止开发协调，在 Axum 等待退出前取消开发 WebSocket，再等待已接受发布任务，回收 Vite、关闭 JavaScript worker 与 SQLite。调用者取消不会取消已经接受的数据库发布任务；写入结果不明仍须核对，不能自动重试。

## 技术与依赖

Rust 2024 与双 Crate 架构保持不变。正式依赖为 notify 8.2.1、reqwest 0.13.5（关闭 default features）、tokio-tungstenite 0.29.0（关闭 default features，启用 connect/handshake）、futures-util 0.3.34（sink），Axum 0.8.9 启用 ws。WebSocket 客户端与 Axum 使用同一 tokio-tungstenite 主版本，避免额外重复版本。

依赖来源及校验值保留于 Cargo.lock，使用工程 Cargo 缓存；前端沿用锁文件中的 Vite 8.3.5、React 插件 6.1.2、Rolldown 1.2.16 等版本。Vite 开发服务器、React Refresh 与 Rust 代理分别实现，不把 Rolldown 构建能力等同于完整 HMR。没有修改 KEngine 的 Web 业务边界。

## 限制与后续验证

当前仅运行可信项目代码；QuickJS 限额和开发镜像不是操作系统级不可信代码沙箱。生产产物哈希用于完整性验证，不是对不可信发布者的签名认证。持有宿主或部署文件写权限的操作者仍在可信管理边界。

SQLite 提交与磁盘清单激活不构成跨介质事务。提交后的镜像失败回退到已验证静态资产，清单激活失败明确报告收尾状态；断电与进程崩溃故障注入尚未全面验收。notify 当前使用 RecommendedWatcher，没有自动轮询回退；旧版本回收和多进程开发协同不在当前范围。

阶段 09 全 Workspace 最终门禁和自定义 HMR WebSocket 的真实 CLI Ctrl+C 退出已完成。下列定向结果保留各自范围；最终统一结果为 201 项常规 Rust、2 项显式跨工具与 90 项前端测试通过，六套浏览器流程通过。具体命令证据见 [阶段 09 验证](../verification/0011-cli-lifecycle.md)及[最终验收](../verification/0012-final-acceptance.md)。

## 执行验证记录

环境为 Windows PowerShell、Rust 1.99.1、MSRV Rust 1.98.0、Node 24.22.0 与真实 Chromium 156.0.8078.4。以下统一门禁和浏览器结果由主执行代理实际执行后交接；本记录编写时再次核对了对应脚本、测试与实现，没有重新运行全部浏览器任务。

| 执行项目 | 实际结果与边界 |
|---|---|
| `npm test --prefix crates/kenvoy/frontend`、`npm run check --prefix crates/kenvoy/frontend`、`npm run build --prefix crates/kenvoy/frontend` | 90 项客户端测试通过，类型检查与构建退出 0 |
| artifact 定向测试 | 2 项通过，退出 0，覆盖产物校验、失败保留及输入变化失效 |
| KEngine 独立测试 | 66 项通过，退出 0；未将其与尚在重跑的 Workspace 数量合并 |
| `$env:CARGO_INCREMENTAL='0'; cargo +1.98.0 check -p kengine --offline --locked` | 34.08 秒，退出 0 |
| `cargo test -p kenvoy --bin kenvoy development_ --offline --locked` | 2 项真实回环 HTTP/WS 测试通过，退出 0 |
| 首导航发布屏障定向测试 | 单项通过；等待不持有模型读锁，发布后使用新权限，超时为 503 |
| `cargo clippy -p kenvoy --bin kenvoy --offline --locked -- -D warnings` | 代理与首导航屏障实施后的定向严格检查退出 0；不替代最终 Workspace 门禁 |
| `node crates/kenvoy/frontend/scripts/extensions-e2e.mjs` | 真实 HMR 通过：数量 2 经组件修改仍为 2，继续操作变为 4；CSS 更新、语法错误及修复后继续保持 4；函数调用及客户端私有代码隔离仍通过 |
| `node crates/kenvoy/frontend/scripts/watcher-e2e.mjs` | 退出 0：真实 notify、语法错误保留输入/模型/活动清单、安全增字段、阻断删字段、修复恢复和真实数据保留 |
| `node crates/kenvoy/frontend/scripts/production-e2e.mjs` | 退出 0：run 无预构建启动、源码变化重建、共用数据库；移走源码与缓存并从 PATH 去掉 Node 后仍加载产物，账户管理可用，开发接口 404 |
| 最新二进制前台、字段与后台浏览器脚本及 `node tools/tests/http-cli.mjs` | 主执行代理已完成复验，全部退出 0；HTTP 脚本 20 项真实断言通过 |
| 基础 DSL 真实 PTY Ctrl+C 与 shutdown-observer | 已观察关闭日志、服务端口断开，重启后管理后台仍能查询原记录；这项证据不等同于自定义 HMR 连接退出已通过 |

联调中实际遇到并修正了 Windows 主线程异步栈过大（对嵌套大 Future 使用 Box::pin）、首导航依赖优化竞态、Runtime/SDK 循环使 Context 被重建、开发路径 @id/raw 越过镜像、产物输入指纹遗漏扩展名等问题。修正后真实 HMR 的 Hooks、CSS 和错误恢复状态保持通过，没有通过取消断言或关闭检查掩盖失败。

本轮首次 Workspace 测试还发现 frontend::tests 中非 canonical Windows 根路径导致资产变更被误分为 Boundary，而非 Corrupted；verify 入口统一 canonicalize 后，定向与完整 Workspace 复验通过。本记录保留首次失败，最终退出码和剩余验证由关联验证记录保留。
