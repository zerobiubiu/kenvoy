# 0001：阶段 00 技术探针执行报告

状态：已执行；日期：2026-11-01；基线：main / b381cba。

## 授权与范围

用户明确授权：“我同意，不需要等我审批，你可以自主决定”。据此自主准备本工程必要依赖和技术验证，不再等待逐包批准；不扩大为业务数据破坏、对外发布或任务无关安装。根 AGENTS.md 已同步该项目授权。

本次完成阶段 00 的最小兼容性探针，不是正式解析/应用运行时实现。依赖位于现有 Cargo 缓存及 tools/probes/frontend/node_modules；Rust 输出位于 target，前端输出和浏览器证据位于忽略目录。Workspace 成员显式固定为两个，未新建第三个 Rust Crate，未提交或推送。

## 实际结果

| 探针 | 结果 | 证据与限制 |
|---|---|---|
| P01 | 6 个测试通过 | JS/TS/JSX/TSX、有限 JSX 转换、Fragment/spread/动态表达式保留、直接引用文本、相对 import 夹具、Unicode/CRLF 位置、恢复错误拒绝、JSON 往返 |
| P02 | 9 个测试通过 | 真 TS 去类型、跨文件白名单 ESM、JSON、Promise/Future、异常、interrupt、未完成 Promise deadline、2 MiB 内存限制、宿主能力隔离和释放 |
| P03 | 2 个测试通过 | 真 SQLite 文件、WAL/FK/timeout、事务/DDL 回滚、迁移重放、并发与备份；Axum loopback 同端口 API/静态 shell/资源、错误状态与授权门控 |
| P04 | 7 项浏览器检查通过 | 真 Chromium、描述→React/MUI、Props/Hooks/CSS、生产构建与深层路由、标记扫描/导入拒绝、Fast Refresh 和错误恢复 |
| P05 | 1 个测试通过 | 真 Windows notify watcher、四模块缓存、反向失效、原子保存、重复内容 no-op、错误候选保留/修复、CSS/配置/静态资源和释放 |

Rust 总计 18 个探针测试通过，lib/main 当前仍为原空骨架。P01 的测试类型不是正式 Generic IR；没有完整 SymbolId/作用域绑定、重导出、npm 包解析或全部表达式转换。P05 的代次和模型是 test-local，不是已经上线的应用热更新系统。

P03 身份门控是探针专用值，不是管理员 Session/CSRF；固定 Schema 与固定迁移不等于正式 Schema Diff 引擎。P04 页面 JSON 由测试提供，不是 KEngine 输出，其表单没有连接数据库。P03 的 HTML shell 不冒充 React。上述事实不能组合为“customer-app 已完成”。

## 环境与锁定依赖

Windows / PowerShell；rustc/cargo 1.99.1，host x86_64-pc-windows-msvc；Node v24.22.0，npm 12.2.0；已有 Chromium 156.0.8078.4，没有额外下载浏览器。实际使用已安装 VS 18 Community 的原生构建链，SWC stacker、QuickJS 和 bundled SQLite 编译成功；未独立测试 MSRV 1.98。

Cargo 精确直接版本与 feature 见根 Cargo.toml，本次 cargo fetch 锁定 203 个外部包；来源和校验值见 Cargo.lock。阶段 00 依赖仅挂到现有两包的 dev-dependencies，正式库仍无 Web/数据库依赖。

实际 bundled SQLite 为 3.54.1；WAL=wal、Foreign Keys=1，忙锁测试 timeout=150ms，观察到 DatabaseBusy。四个独立写者写入 40 条，重开后含基准记录共 41 条；HTTP 六个并发写入后重开共 8 条，备份 API 与临时目录清理通过。

rquickjs/rquickjs-sys 为 0.14.0，std/loader/futures，无 rust-alloc/dyn-load；发布包 quickjs.h 标明 QuickJS-NG 0.16.2。rquickjs 发布源码仓库 SHA 为 d7ef5eeae702fea24c03643064de454f1c1dd4b0，不是底层子模块 commit。底层 gitlink 查询受 GitHub API 限流，状态码未保留；未虚构该 commit。补充源码 SHA256：

```text
quickjs.c 3A6B52A225C21709EF1FD314183EFD3AF862E867CE4EF8CAABCAE67922A745B3
quickjs.h 979127138DA79CAD5DDC7EFFA1AFB13E8B02F744DA2AA36844D4CB9D8CB92DE9
```

前端锁定 React/DOM 19.3.1、MUI 9.5.0、Emotion 11.14.0/11.14.1、Router 8.5.1、Vite 8.3.5、React 插件 6.1.2、Rolldown 1.2.16、Playwright 1.64.1。package-lock.json 记录来源和 integrity，npm ci 重放 105 包。安装使用 --ignore-scripts，已有平台 binding 实际可用。

## 失败与修正

- P01 首跑 4/6：JSX 属性表达式内的 JSXElement 需保留节点；含 emoji 的列号需从源码字节区间显式转换为 Unicode 标量列。修正节点转换及诊断/节点同一列规则后 6/6。
- P02 首跑 8/9：完全耗尽内存时引擎可能只能抛 null。改为一次申请 16 MiB 超过 2 MiB 限额，保留错误对象空间，严格断言 out of memory 且 interrupt=false；没有关闭限额或断言。
- P03 首次编译因 rusqlite 新版移除 DatabaseName 失败，按实际 API 改为 MAIN_DB/NO_PROGRESS；Clippy 两个 collapsible_if 修正后复验。
- P04 首跑因同进程 build 保留 NODE_ENV=production，dev 插件禁用 Refresh，计数 3 变 0。修正为启动 dev 前设置 development 并 finally 恢复环境，计数在组件编辑、CSS 和语法修复后均为 3。失败 JSON 与最终结果保留在本机 .probe/，不作为发布资产。
- P05 首次 unused import 被移除，定向 Clippy 复验通过。

## 阶段结论与后续

所选默认技术路线具备本机最小集成可行性，阶段 00 的架构审查、设计和核心技术探针门禁通过，可以进入阶段 01。正式跨 Crate 公共 API、完整业务模型、安全会话、迁移分析和整体 HMR 的功能验收仍在后续阶段，不能因本报告而关闭所有风险。

阶段 01 从独立 KEngine 公共 API、通用 IR 和基本模块分析开始，先完成自身测试，再验证 Kenvoy 只通过公共 API 消费；不在同一阶段实现完整 HTTP、数据库和前端业务。完整作用域/符号和增量分析留到阶段 02。

## 执行验证记录

环境和版本见上文；原有未跟踪 AGENTS/docs 均保留，无 Git 提交。

| 实际命令 | 退出码 | 输出摘录与结论 |
|---|---|---|
| cargo fetch | 0 | Locking 203 packages；保存 Cargo.lock |
| cargo test --workspace --no-run --locked | 0 | 原生依赖编译成功，22.73s；仅为编译验证 |
| cargo test -p kengine --test p01_language --locked -- --nocapture | 首次 1，修正后 0 | 6 passed / 0 failed |
| cargo test -p kenvoy --test p02_javascript --locked -- --nocapture | 首次失败退出码未保留，修正后 0 | 9 passed / 0 failed |
| cargo test -p kenvoy --test p03_http_sqlite --locked -- --nocapture | 首次编译失败退出码未保留，修正后 0 | 2 passed / 0 failed |
| cargo test -p kenvoy --test p05_watcher --locked -- --nocapture | 0 | 1 passed / 0 failed；ReadDirectoryChangesWatcher；generation=7 |
| npm.cmd ci --ignore-scripts --no-audit --no-fund | 0 | 在 tools/probes/frontend 执行；105 packages |
| npm.cmd run build | 0 | 981 modules transformed；manifest/source map 已生成 |
| npm.cmd run probe | 首次 1，修正后 0 | 7 项检查 passed，组件计数保留为 3 |
| node --check scripts/probe.mjs；node --check vite.config.mjs | 各 0 | 前端探针脚本语法通过 |
| cargo check --workspace --locked | 0 | workspace check 通过 |
| cargo test --workspace --locked | 0 | 18 个探针测试通过，0 失败；lib/main/doc-tests 各 0 tests |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 | 无告警 |
| cargo fmt --all -- --check | 0 | 格式通过 |

Rust 统一验证临时设置 CARGO_INCREMENTAL=0，未改用户或项目 Cargo 配置。浏览器探针未调用不可用的 ego-browser；使用已获授权的 Playwright，finally 恢复源码并关闭浏览器/服务。后续建议增加 Axum 托管真实 React 资产的跨工具验证，以及在阶段 01/02 覆盖上述解析和模块限制。
