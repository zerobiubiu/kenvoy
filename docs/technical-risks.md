# 技术风险与验证门禁

状态：现行风险清单；更新日期：2026-11-01。阶段 00—10 基础框架验收已通过，阶段 10 已在同一最终构建完成综合验证；以下保留真实未覆盖边界。

## 风险登记

“高”表示影响数据、权限、运行边界或核心闭环，“中”表示影响扩展和开发体验。正式代码存在不等于风险永久关闭；表内区分实际控制、已有验证和仍需验证的条件。历史探针与阶段报告保留原始事实，不改写为当前全量验证。

| ID | 优先级/责任模块 | 当前风险 | 已实施控制与证据 | 真实剩余边界 |
|---|---|---|---|---|
| R01 | 高/KEngine | SWC 变化或 IR 转换遗漏 | 自有版本化 IR、UTF-16/raw/span、结构化表达式及未知引用；阶段 02 正式语义/引用/增量已验收 | 不是完整 TypeScript 类型检查器；升级 SWC 后重跑保真与作用域回归，不以可解析替代类型安全 |
| R02 | 高/模块分析 | 源码解析与 npm 解析不同 | 正式相对/index/.diu、类型边、循环、import/export star、重导出及歧义诊断；运行引用保留门面 | 不实现全部 tsconfig paths、npm exports/条件解析；external 不证明安装/导出有效，用户包由客户端构建处理 |
| R03 | 高/DSL | 动态 Schema 或 UI 被误当数据库声明 | 正式静态常量/对象数组展开、.diu 组合、循环/节点限额、重复与动态诊断；UI 函数体不产字段 | 动态 map、未知结构和任意函数生成 Schema 明确拒绝，不提供完整 JS 静态执行 |
| R04 | 高/JavaScript | TS/Node/npm 兼容被过度承诺 | 阶段 08 正式 TS→JS、静态 ESM、登记、JSON/Promise、异常隔离与 HTTP Action 通过 | 服务端仅本地静态纯计算；无 Node/Bun/npm、文件/网络/数据库 Host API；复杂切片需拆到独立 UI 模块 |
| R05 | 高/JavaScript | 执行限额被误当安全沙箱 | interrupt、deadline、有界队列、独立上下文、栈/内存限额；CPU/微任务/OOM 独立看门狗测试通过 | untrusted 明确拒绝；尚无操作系统隔离。改变 allocator、feature 或平台必须重新测限额 |
| R06 | 高/客户端构建 | Server 经门面/动态引用/资源 URL 泄漏 | 引用边界、捕获切片、私有图、Vite 加载再校验、CSS 资源边界；08 浏览器验证私有标记不入 client，服务端模块 HTTP 404 | npm 工具链处理可信项目，不是恶意构建插件沙箱；新增入口/资源类型须扩充边界测试 |
| R07 | 高/迁移 | 删除/改型/漂移误判丢数据 | 正式 Diff、版本/hash 链、实际结构快照、确认 checksum、backup API、预检、回滚及失败保留通过 | 重命名候选可检测，映射执行尚未支持；大表空间/耗时、断电/磁盘耗尽和自动恢复命令未验收 |
| R08 | 高/数据库 | 连接共享、队列积压及长事务 | 独占连接线程、64 项队列、WAL/FK/5 秒 busy timeout；CRUD、过载、取消、重开和 48 库/96 线程压力回归通过 | SQLite 单写者；高负载与长迁移尾延迟仍需测量。历史单次 Busy 原因未确证，不能把夹具修复当作已证明根因 |
| R09 | 高/SQLite 内核 | 系统链接或发行目标版本变化 | 本机实际核验 bundled SQLite 3.54.1，锁定 rusqlite + bundled/backup | 切换系统库/发布目标须重新查询内核、feature 和修复范围；不把 WAL 文件放网络共享盘作为分布式方案 |
| R10 | 高/认证授权 | 越权、CSRF、旋转或信息泄漏 | 正式 Argon2id、服务端 Session、旋转事务、HttpOnly、Origin/CSRF、单策略字段授权、查重/限流及后台 E2E 通过 | 尚无行级所有权、多租户、MFA、密码找回、多实例限流或独立安全审计；user 角色不等于租户隔离 |
| R11 | 高/HTTP 与静态服务 | API fallback 或静态目录越界 | 注册表路由、404/405、只开放 client、路径/链接验证和身份投影；浏览器/API 通过；09 受控开发代理、首次资源和 WebSocket 已实测 | 反向代理/TLS 部署须按实际 public_origin 验收；不能由本机开发代理结果推定全部部署环境安全 |
| R12 | 高/发布生命周期 | DB 提交与模型/资产切换不同步 | publish_prepared 在同一写锁校验登记/迁移后同步替换模型、JS、assets、generation；取消与失败发布有正式回归；09 真实 watcher 错误保留与修复恢复通过 | 提交后磁盘或代理收尾失败尚未完成故障注入；DB 提交后不能声称回滚，须明确恢复或重启路径 |
| R13 | 中/开发前端 | 所有变更都被当作可保留状态 | 正式受控镜像/Vite/Axum 代理；09 真实宿主组件文案、CSS 更新、错误保留与修复均验证 Hooks 状态保持 | 混合/不兼容导出传播更新，可能重挂或整页刷新；未穷尽所有 Refresh 形态，不承诺任意组件状态永存 |
| R14 | 中/React 与 UI | 双 React、错误 Hooks 调用、手机入口不可见 | 框架依赖统一解析和类型注册；08 Hooks/Props/MUI/clsx/CSS 浏览器通过；手机固定管理菜单已复验、视觉 ready | 单个第三方库不能证明全部生态；Safari/Firefox、实体手机与完整辅助技术矩阵未验收 |
| R15 | 中/notify 与增量 | 原子保存、风暴、断连或漏事件 | KEngine 正式 AnalysisSession、前后图反向失效及全量等价回归；真实 notify 已验证语法错误保留、安全增列、删除阻断与修复；最终 watcher 的配置恢复和详细日志回归通过 | 编辑器/网络盘行为、事件风暴、长期负载与所有不必要重建场景未覆盖 |
| R16 | 高/构建部署 | 输出覆盖、旧产物或篡改启动 | 正式独占 staging、版本目录、manifest、来源/内容指纹、归属/摘要复核；真实 run 自动构建、源码变化重建、无源码/无 Node 仅产物运行与持久化通过；最终同一二进制六套浏览器流程全部通过 | 已发布版本不自动清理；磁盘满、断电尚未故障注入 |
| R17 | 高/依赖与平台 | 锁定不保证各平台可发行 | Cargo/npm 精确版本、来源/hash/lock；Windows 原生构建与严格 clippy 通过；最终修订的 Rust 1.98 Workspace 全目标 check 和格式检查均退出 0 | 最低版本完整运行测试、macOS/Linux、签名/打包、许可证与发布供应链未完成 |
| R18 | 高/资源释放 | HTTP/DB/JS/子进程/WS 退出挂起 | 正式关闭、取消和有界等待回归；09 基础及自定义应用真实 Ctrl+C 后关闭日志、端口释放、当前文档 WS 1→0、无项目残留 Node、重启 SQLite 记录保留 | 构建/发布中途 Ctrl+C 与其他操作系统未验收；PTY 外层退出 1 与原生资源清理分开解释；两处已停止临时夹具因自动审查阻止清理而保留 |

## 已确定的选择与明确未实现项

双 Crate、单向依赖、React/TypeScript/原生 CSS/CSR、Axum/Tokio、SQLite/rusqlite 和 SWC/rquickjs 纯计算路线已经进入正式实现。认证参数、客户端投影、Session/CSRF、迁移确认与备份不再是待设计项，分别见[安全契约](security-design.md)、[数据库设计](database-design.md)和[JavaScript Runtime](javascript-runtime.md)。

当前未实现完整 TS 类型检查、任意动态 Schema、服务端 npm/Node 兼容、数据库/文件/网络 Host API、不可信代码操作系统隔离、重命名映射执行、关系/上传字段、行级多租户与多框架适配。必须通过诊断或文档明确边界，不能静默忽略或由“解释执行”推导为已支持。

基础 DSL、正式 CRUD/认证/后台和扩展 E2E 已验证；性能上限、长时稳定性、跨平台与所有异常恢复不会因此自动通过。第一版状态以[路线](development-roadmap.md)与最新正式记录为准。

## 阶段 09 已取得的证据与最终门禁

1. 真实 Rust 开发入口已验证首次资源、Refresh preamble、WebSocket、普通兼容组件 Hooks 状态、CSS、编译错误保持与修复；混合导出降级仍保留明确边界。
2. 候选失败保留有效模型和数据库、安全新增字段与破坏性删除阻断已有真实 watcher 验证；提交后故障恢复不视为已覆盖。
3. 未预先 build 的 run、源码变化重建、仅产物且无 Node 的启动、dev/run 同一 SQLite 和重启持久化已有真实验证；产物篡改拒绝有独立回归。
4. 基础与自定义应用完成真实 CLI Ctrl+C、关闭日志、端口释放、当前文档 WebSocket 归零及重启保留记录；不能将这些证据扩大为构建中途退出或全部故障路径。

最终 Rust 有 201 项不重复常规测试通过；常规运行中忽略的 2 项跨工具验证随后显式执行并通过。Workspace 全目标 check、严格 clippy（`-D warnings`）、build、最终修订的 Rust 1.98 Workspace 全目标 check 与 fmt 检查均退出 0。同一最终二进制的 forms、fields、admin、watcher、production、extensions 六套浏览器流程和 20 项真实 HTTP 断言全部通过。命令、二进制 SHA-256、失败修复及限制见[生命周期验证](verification/0011-cli-lifecycle.md)与[阶段 10 最终验收](verification/0012-final-acceptance.md)。这完成第一版基础框架验收，不消除表内未覆盖风险；修改既定路线必须新增 ADR，不把障碍转移为 KEngine 的业务职责。

## 资料与验证依据

正式证据见[阶段 02](verification/0004-kengine-semantics.md)、[阶段 03](verification/0005-dsl-cli.md)、[阶段 04](verification/0006-sqlite-runtime.md)、[阶段 05](verification/0007-http-auth.md)、[阶段 06](verification/0008-react-forms.md)、[阶段 07](verification/0009-admin-browser.md)、[阶段 08](verification/0010-typescript-extensions.md)、[阶段 09](verification/0011-cli-lifecycle.md)与[阶段 10](verification/0012-final-acceptance.md)。阶段 00 的[技术探针](verification/0001-stage00-probes.md)和[同源浏览器探针](verification/0002-axum-react-browser.md)保留选型历史，不替代正式结果。

官方能力和限制链接保留在[技术栈](technology-stack.md)，关键依据为 [rquickjs Runtime](https://docs.rs/rquickjs/latest/rquickjs/struct.Runtime.html)、[Tokio 阻塞任务](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)、[SQLite WAL](https://www.sqlite.org/wal.html)、[Rolldown 能力](https://rolldown.rs/guide/introduction)、[React 插件](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react/README.md)与 [Vite 后端集成](https://vite.dev/guide/backend-integration)。
