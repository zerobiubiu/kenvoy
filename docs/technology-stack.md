# 技术栈与可行性调查

状态：现行；阶段 00—10 基础框架验收已通过，阶段 10 使用同一最终构建完成综合验证；更新日期：2026-11-02。

## 证据等级与本机条件

本文区分已落正式实现、真实测试与仍待验收的能力。阶段 00 探针保留为选型依据，不能替代正式功能验证；命令、退出码、失败与限制见[验证索引](verification/README.md)，历史报告不随本现状文档改写。

实际验证环境为 Windows/PowerShell、Rust/Cargo 1.99.1、Node 24.22.0、npm 12.2.0、Chromium 156.0.8078.4 与 VS 18 Community 原生构建链。Workspace 使用 Rust 2024，声明 MSRV 1.98；除阶段 01 的 KEngine 独立历史验证外，本轮 `cargo +1.98.0 check --workspace --all-targets --offline --locked` 已退出 0，最终修订也已复核，覆盖完整宿主和测试目标的编译。该检查不等于在最低版本上完成所有运行测试。macOS/Linux、其他浏览器和实体移动设备尚未完成相同验收。

阶段 02 已验收正式语义和增量会话；03 验收 DSL/Application IR；04 验收 SQLite、迁移和 CRUD；05 验收 HTTP/认证；06/07 验收 React 登记与后台；08 验收用户组件、受限服务端 JavaScript 及跨端边界。阶段 08 统一记录为 196 项常规 Rust、86 项客户端及真实扩展浏览器验收；后补 JavaScript 和开发状态的 89 项客户端测试分别记录，不合并冒充一次统一门禁。

最终门禁取得 201 项不重复的常规 Rust 测试通过；常规运行中忽略的 2 项跨工具验证随后通过 `--ignored` 显式执行，均退出 0。Workspace 全目标 check、严格 clippy（`-D warnings`）、build 和最终 fmt 检查均退出 0；JavaScript 看门狗子进程不重复计入用例数。真实 HMR、生产自动构建与仅产物运行、基础及自定义组件的 Ctrl+C 退出证据见[阶段 09 生命周期验证](verification/0011-cli-lifecycle.md)。同一最终二进制的 forms、fields、admin、watcher、production、extensions 六套浏览器流程及 20 项真实 HTTP 断言全部通过，watcher 包含配置恢复与详细日志回归，见[阶段 10 最终验收](verification/0012-final-acceptance.md)。

## 已锁定的直接依赖

版本与 feature 来自当前根 manifest、各成员 manifest、Cargo.lock 及 crates/kenvoy/frontend/package-lock.json。Rust 使用精确版本和 crates.io 校验值；npm 直接版本、锁文件和 registry 来源已逐项核对。独立 tools 前端探针已退役；工程检查由 Rust xtask 承担，正式浏览器流程由 kenvoy-tests 管理。

| Cargo 直接依赖 | 精确版本 | 当前用途与显式 feature |
|---|---|---|
| swc_common | 27.0.0 | 源码、span；默认 |
| swc_ecma_ast、swc_ecma_visit | 各 30.0.0 | AST 与遍历；默认 |
| swc_ecma_parser | 47.0.0 | JS/TS/JSX/TSX；默认 |
| swc_ecma_codegen | 33.0.0 | Kenvoy 扩展转译；默认 |
| swc_ecma_transforms_base | 51.0.0 | 通用 resolver 与 Kenvoy fixer；默认 |
| swc_ecma_transforms_typescript | 57.0.0 | Kenvoy 消除类型；默认 |
| serde、serde_json、thiserror | 1.0.229、1.0.154、2.0.21 | serde 启用 derive；其余默认 |
| rquickjs | 0.14.0 | default-features=false；std、loader、futures |
| axum | 0.8.9 | 默认 + ws；正式 HTTP 与开发 WebSocket |
| tokio | 1.54.1 | macros、rt-multi-thread、net、io-util、sync、time、signal、process |
| tower、tower-http | 0.5.3、0.7.1 | 分别启用 util；fs、limit |
| rusqlite | 0.41.1 | bundled、backup |
| notify | 8.2.1 | 正式开发文件监听；默认 |
| clap、toml | 4.6.7、0.9.8 | clap 启用 derive；严格 CLI/配置 |
| chrono | 0.4.45 | default-features=false；std |
| sha2、hmac | 0.11.1、0.13.0 | 产物/Schema/Session 摘要与 CSRF |
| argon2、getrandom、rpassword | 0.6.0、0.4.3、7.5.4 | 密码哈希、系统随机源、隐藏密码输入 |
| tracing、tracing-subscriber | 0.1.44、0.3.23 | subscriber 启用 env-filter、json |
| url | 2.5.8 | 公共 Origin 等 URL 边界 |
| reqwest | 0.13.5 | default-features=false；受控回环开发代理 |
| tokio-tungstenite | 0.29.0 | default-features=false；connect、handshake |
| futures-util | 0.3.34 | 默认 + sink；开发代理流处理 |
| windows-sys | 0.61.2 | 仅测试成员 cfg(windows)：Foundation、Security、JobObjects、Threading；回收自有进程树 |

产品为 kengine Library 与 kenvoy Binary，Workspace 另有不发布的 kenvoy-tests 与 xtask。KEngine 正式依赖仅为通用 SWC、serde 和 thiserror，不引用 Web/数据库/React/认证。上述 Web、SQLite、QuickJS、notify 等已经是 Kenvoy 正式依赖；sha2 同时用于资产嵌入构建脚本。测试成员的 Windows Job 依赖不进入产品正常依赖闭包。

| npm 直接依赖 | 精确版本 | 正式前端用途 |
|---|---|---|
| react、react-dom | 各 19.3.1 | 通用 CSR Runtime |
| @radix-ui/react-slot、clsx | 1.5.0、2.1.1 | 仅开发测试夹具与用户示例依赖；不进入默认 Runtime |
| react-router | 8.5.1 | Declarative Router |
| vite、rolldown | 8.3.5、1.2.16 | 锁定开发/构建适配与打包内核 |
| @vitejs/plugin-react | 6.1.2 | React 转换与 Fast Refresh |
| typescript | 7.0.2 | 框架前端 tsc --noEmit |
| @types/react、@types/react-dom | 各 19.3.0 | 框架前端类型 |
| @playwright/test | 1.64.1 | 真实浏览器验收 |

默认 Runtime 的生产 npm 依赖仅 React、React DOM 和 React Router。其余是构建或测试工具；应用自己的组件库由应用 package.json 与锁文件管理。MUI/Emotion 已从框架及探针依赖移除，选择原因见 [ADR 0010](adr/0010-native-components-and-data-bindings.md)。

## 正式实现与技术边界

### SWC、通用 IR 与业务解释

KEngine 已实现四种语法、UTF-16 保真字符串、表达式结构、变量 initializer、源码追踪、值/类型作用域与符号、跨模块链接、歧义诊断和 AnalysisSession 内容缓存/反向失效。Parser 同时收集恢复错误。公共契约是独立版本化 Generic IR，不公开 SWC AST 作为长期接口；详见 [KEngine 设计](kengine-design.md)和[阶段 02 验证](verification/0004-kengine-semantics.md)。[SWC Parser](https://docs.rs/swc_ecma_parser/latest/swc_ecma_parser/)、[JSX AST](https://docs.rs/swc_ecma_ast/latest/swc_ecma_ast/struct.JSXElement.html)

Kenvoy 已有正式静态求值、.diu.tsx 展开、Application IR、字段约束/路由/权限检查。普通 UI 函数体不推导 Schema，运行引用保留 import 门面；无法静态确定的 Schema 明确诊断。两层均不是完整 TypeScript 类型检查器，可解析不代表任意程序可静态执行。KEngine 的 external 不证明 npm 包已安装或导出有效；客户端 npm 解析由受控构建工具完成。

### React、原生 CSS 与用户组件

通用 React Runtime 消费经权限过滤的应用描述，提供七类字段、前台登记、登录与后台搜索/排序/分页/详情/增改删，提交经同源 API 写入真实 SQLite。默认使用语义 HTML 与 CSS；自定义组件按类型登记，Hooks 交给 React。构建器统一 React 基础包解析以避免重复实例，用户 UI 库按项目依赖解析。阶段 08 的原 MUI 验证保留为[历史证据](verification/0010-typescript-extensions.md)。[createElement](https://react.dev/reference/react/createElement)、[createRoot](https://react.dev/reference/react-dom/client/createRoot)、[重复 React 风险](https://react.dev/warnings/invalid-hook-call-warning)

原生控件与 React Router Declarative Mode 统一消费客户端描述。前端使用 Fetch、HttpOnly Session 与内存 CSRF/代际，不以 LocalStorage 作为认证依据。系统重载通过同一字段绑定操作模型，见[组件与选项契约](component-bindings.md)。[React select](https://react.dev/reference/react-dom/components/select)、[Router 模式](https://reactrouter.com/start/modes)

### Vite/Rolldown 与开发更新

正式客户端构建采用 Vite + Rolldown + React 插件；TS/JSX 转换不承担完整类型检查，不假设 Rolldown 单独提供 Vite 开发服务器。生产 HTTP 始终由 Axum 提供。基础 DSL 资产预先嵌入 CLI，`frontend::export` 写入独占空 staging，统一产物默认保存到 `dist/versions/<build_id>/client`，`dist/manifest.json` 选择有效版本。[Rolldown 简介](https://rolldown.rs/guide/introduction)、[Getting Started](https://rolldown.rs/guide/getting-started)、[Vite 指南](https://vite.dev/guide/)

阶段 09 已接入受控源码镜像、Vite 开发子进程、Axum 资源/WebSocket 代理和开发状态轮询；先校验候选，再向镜像和有效模型发布。真实宿主浏览器已验证首次资源加载、普通组件修改和 CSS 更新保留 Hooks 状态、错误 TSX 保留有效页面及修复恢复。基础应用与自定义组件应用均完成真实 Ctrl+C、端口释放和重启 SQLite 记录保留；自定义页面当前文档的活动 WebSocket 从 1 归零，未见该项目残留 Node。证据及 PTY 外层退出码解释见[阶段 09](verification/0011-cli-lifecycle.md)，不是早期 P04 独立 Vite 探针的替代推断。React 插件对不兼容或混合导出会使更新向父模块传播，部分变化可能重挂或整页刷新；不承诺任意编辑都保留状态。[React 插件](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react/README.md)、[Vite 后端集成](https://vite.dev/guide/backend-integration)

### QuickJS 纯计算运行时

Kenvoy 已实现 SWC resolver/strip/fixer/codegen、版本化静态 ESM 包、函数登记、白名单 loader、有界工作线程、每次调用独立 Runtime/Context、同步/Promise 与严格 JSON。真实测试覆盖异常、队列、取消、无限循环/微任务中断、未完成 Promise 与 2 MiB OOM，见 [JavaScript Runtime](javascript-runtime.md)。源码路径和服务端模块不随客户端投影公开。[rquickjs](https://docs.rs/rquickjs/latest/rquickjs/)、[AsyncRuntime](https://docs.rs/rquickjs/0.14.0/rquickjs/struct.AsyncRuntime.html)

rquickjs/rquickjs-sys 0.14.0 发布包内核在阶段 00 核对为 QuickJS-NG 0.16.2，源码哈希留在历史报告；不虚构未取得的 gitlink commit。未启用 rust-alloc 或动态原生加载；改变 allocator/feature 必须重测内存限制。只开放可信项目的本地静态纯计算，无 Node/npm 服务端兼容层及文件、网络、数据库 Host API；runtime.trust=untrusted 明确拒绝，当前不是操作系统沙箱。[内核引用](https://github.com/DelSkayn/rquickjs/blob/master/.gitmodules)、[Runtime 限制](https://docs.rs/rquickjs/latest/rquickjs/struct.Runtime.html)

### SQLite、迁移与 HTTP

正式 worker 独占 rusqlite Connection，队列容量 64，启用 WAL、Foreign Keys 和 5 秒 busy timeout；不在 Tokio 请求线程无约束共享连接。实际 bundled 内核已核对为 SQLite 3.54.1。WAL 仍有单写者约束，不能当作分布式/网络文件系统数据库方案；切换系统 SQLite 或升级时重新核验实际内核及 WAL 修复范围。[rusqlite Connection](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html)、[SQLite WAL](https://www.sqlite.org/wal.html)、[Tokio 阻塞任务](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)

阶段 04 已完成通用 Schema Diff、确认 checksum、历史、结构漂移检查、backup API、数据预检、事务回滚和重复迁移空操作。新增表/安全列/索引按策略应用；破坏性变更默认阻断，显式确认也不能跳过数据校验。候选重命名可识别，但尚无映射执行；大表、磁盘耗尽和断电未专项验收，见[数据库设计](database-design.md)。

Axum 已正式提供注册表驱动 CRUD、Action、认证、授权与静态资源。Argon2id 初始化、服务端 Session、HttpOnly/SameSite Cookie、Origin/CSRF、表单字段范围、输入查重与分页/请求限额已经验证。只服务版本产物 client，API 错误不进入 SPA fallback，modules/server.json 不公开。详见[安全契约](security-design.md)与[阶段 05](verification/0007-http-auth.md)。[Axum](https://docs.rs/axum/latest/axum/)、[ServeDir](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html)、[notify](https://docs.rs/notify/latest/notify/)

## 依赖管理与后续门禁

用户已明确授权工程必要依赖自主准备，不扩大为无关安装、费用、对外发布或破坏用户数据。框架编译依赖、内置资产、用户 npm 和 E2E 工具分别管理。基础 DSL 启动不需要 Node/npm；自定义工具链按锁文件/环境复用，首次准备采用 npm ci --ignore-scripts，用户项目安装须显式 --install-deps，不改写其 package.json，不每次启动重装。服务端 JS 第一版不加载 npm 包。[npm ci](https://docs.npmjs.com/cli/v11/commands/npm-ci/)、[package-lock.json](https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json/)

已完成正式事实以 02—10 报告中的实际结果为准；同一最终二进制的整套浏览器综合验收、最终修订的 MSRV 与格式复核均已通过。完整宿主的 Rust 1.98 全目标编译不替代最低版本的完整运行矩阵；跨平台发行、许可证/发布供应链与不可信代码隔离仍未完成。构建中途退出、磁盘故障及长时负载不在本轮通过范围，见[风险清单](technical-risks.md)和[路线](development-roadmap.md)。
