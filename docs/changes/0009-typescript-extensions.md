# TypeScript 扩展与受限函数执行

状态：已执行；日期：2026-11-01；影响范围：Kenvoy 应用模块、客户端构建、QuickJS、HTTP Action 及构建产物；不改变 KEngine 职责或双 Crate 结构。

## 背景与方案

基础表单及管理后台已运行，但普通组件引用和 Action 仍不能形成完整的用户扩展闭环。本次复用 KEngine 词法绑定与静态模块图，在 Kenvoy 建立客户端注册表和独立服务端函数包；保持通用页面解释，不为每个资源/页面生成业务控制器或独立页面源码。

独立 UI 模块按原 import 门面交给 Vite，入口中的表达式/组件按实际捕获切片，避免将整个应用声明连同 server import 放入浏览器。服务端使用 SWC 消除 TS，逻辑模块 ID 与精确依赖映射交给 QuickJS 工作线程。HTTP Action 使用现有认证、CSRF、权限与代际机制，异常不会暴露私有消息。

## 实施文件

- 新增 `crates/kenvoy/src/javascript/{mod.rs,compiler.rs,source_slice.rs,runtime.rs,runtime/tests.rs}`：独立诊断、版本化 PreparedBundle、转换/复验、客户端切片、有界线程、JSON 与资源限额。
- 修改 `crates/kenvoy/src/dsl/{analyzer.rs,static_eval.rs}`：保留运行 import 来源；修复 Server/Shared 根混用；表达式边界按真实词法依赖检查；新增定向回归。
- 新增 `crates/kenvoy/src/client_build.rs`、`client_build_tests.rs` 与 `frontend/scripts/client-builder.mjs`：受控构建目录、锁定框架缓存、显式用户依赖安装、模块白名单、源码切片虚拟模块、注册表输出。
- 修改 `crates/kenvoy/frontend/src/{Runtime.tsx,types.ts,api.ts,env.d.ts}`，新增 `sdk.ts`，扩展客户端测试：组件/函数/表达式注册、API Action、公开 SDK；前端渲染继续使用通用 Runtime。
- 修改 `crates/kenvoy/src/http/{mod.rs,handlers.rs,tests.rs}`：Action 路由、Execute、严格参数、限流、同代 JS 包发布及脱敏错误。
- 新增 `crates/kenvoy/src/{artifact.rs,build.rs}`，修改 `main.rs/project.rs/error.rs`：构建聚合、私有 server.json、版本化清单与哈希、trust 配置、build/dev/check 接线；完整 run/监听接线随后验收。
- 新增 `crates/kenvoy/frontend/scripts/extensions-e2e.mjs`：真实组件、第三方依赖、Schema、TS ESM/Promise、SQLite 提交及私有代码扫描。
- 修改根及 Kenvoy Cargo manifest/lock：已有 SWC 与 rquickjs 从探针依赖提升为正式执行依赖，Tokio 增加 process；不新增自有 Crate。
- 新增/更新 JavaScript、DSL、Application IR、安全现状文档和本阶段验证、分类索引；没有删除已有实现或重写历史执行记录。

## 技术与依赖

SWC 锁定 common 27.0.0、AST/visit 30.0.0、parser 47.0.0、codegen 33.0.0、transforms_base 51.0.0、typescript 57.0.0；rquickjs 0.14.0，关闭 default features，仅 std/loader/futures；Tokio 1.54.1 增加 process。依赖来自 Cargo.lock 指定的 crates.io 源，在工程 Cargo 缓存与构建目录使用。没有启用会使原生内存限制失效的 rust-alloc，也没有 QuickJS 任意动态原生模块加载。

前端沿用锁定 React 19.3.1、MUI 9.5.0、Vite 8.3.5、React plugin 6.1.2、Rolldown 1.2.16、TypeScript 7.0.2 与 Playwright 1.64.1。扩展验收项目显式安装 clsx 2.1.1；构建/验收不会静默改写用户 package.json，安装结果以锁文件和构建清单追踪。框架缓存与用户项目依赖分开，不在每次启动重新安装全部依赖。

## 执行验证记录

环境为 Windows、Rust 1.99.1、Node 24.22.0、真实 Chromium 156.0.8078.4。主执行代理统一运行 `cargo test --workspace --offline --locked`，196 项常规通过、2 项需额外 Node/npm 或浏览器条件的显式测试 ignored，退出 0；`npm test --prefix crates/kenvoy/frontend` 86 项通过，退出 0；`node crates/kenvoy/frontend/scripts/extensions-e2e.mjs` 真实浏览器闭环退出 0。

后补普通 TS 泛型切片与导出名冲突回归后，执行 `cargo test -p kenvoy --bin kenvoy javascript:: --offline --locked`，18 项通过，退出 0；没有将新增一项冒充前述 196 项全量结果的一部分。Action 集成目标单项、原 DSL 11 项和静态求值 13 项均实际通过。`cargo clippy -p kenvoy --bin kenvoy --offline --locked` 退出 0，保留阶段 09 未消费的 5 组接口警告；本任务文件 rustfmt 退出 0。命令、失败修正、OOM/CPU 独立 watchdog 与边界见 [完整验证记录](../verification/0010-typescript-extensions.md)。

后续验证：dev 文件变化与错误恢复、React 状态保留、配置重载、生产产物复用/重建和 run，无需预 build 的真实重启持久化；不可信代码隔离及受控资源 Host API 留在后续功能范围。当前限制以 [JavaScript Runtime](../javascript-runtime.md) 为准。
