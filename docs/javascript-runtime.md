# TypeScript 扩展与 JavaScript Runtime

状态：现行；核对日期：2026-11-01。本文维护 TS 转换、模块、函数调用和隔离边界，实现位于 `crates/kenvoy/src/javascript/`、`client_build.rs`、`http/` 和通用 React Runtime。构建复用、开发监听、Fast Refresh 及生产产物的验收另见[生命周期报告](verification/0011-cli-lifecycle.md)。

## 编译和执行边界

KEngine 提供通用源码、模块图和词法引用。Kenvoy 独立完成 TS 类型消除、应用模块分类、白名单构建、JS 执行和浏览器注册；没有向 KEngine 增加 React、HTTP 或 QuickJS 依赖。Workspace 仍只有两个自有 Crate。

服务端使用 SWC resolver → TypeScript strip → fixer → codegen，将可信项目的本地静态 ESM 转为 JavaScript。模块 ID 为 `m_1` 等逻辑标识，依赖映射精确到导入者与原始说明符。`PreparedBundle` v1 包含 modules/code/imports 和 functions/module/export；函数键是 Application IR 的 handler 注册 ID。代码与映射可序列化，加载时重新校验版本、大小、语法、导入闭包和导出登记。

最多 256 个模块、1024 个函数登记，单模块源码 512 KiB、转换后代码合计 4 MiB。静态循环依赖可建图；运行时仍遵循 ESM 初始化规则。type-only 导入被剥离，不成为运行期依赖。服务端第一版拒绝 npm/Node/URL/绝对路径模块、JSX、动态 import、import.meta、import attributes、TypeScript import/export =，并保守拒绝 require、eval、Function 标识符。此限制不是完整 ECMAScript 类型检查或操作系统沙箱。

`runtime.trust` 默认为 `project`。显式 `untrusted` 在准备/启动时拒绝，尚未提供不可信项目执行模式。构建产物及本机项目维护者仍属于可信管理边界。

## 有界工作线程

Rust 宿主创建专用线程和 current-thread Tokio 执行器，线程只接收 JSON 参数、不可变模块包及响应通道。每次调用创建全新的 QuickJS Runtime 与 Context，模块代码可以复用，用户全局变量和 JS 对象不跨调用复用；QuickJS 句柄不跨线程。

| 限制 | 默认值 | 行为 |
|---|---|---|
| 调用总期限 | 1 秒 | 从入队开始计时；队列等待不延长期限 |
| QuickJS 内存 | 32 MiB | 使用原生内存限制；未开启 rust-alloc 自定义分配器 |
| JS 栈 | 512 KiB | 单上下文限制 |
| 待执行队列 | 32 项 | 已满立即拒绝，不无限排队 |
| 参数个数 | 32 项 | HTTP 与 Runtime 均检查 |
| JSON 输入/输出 | 各 1 MiB | 输入先检查结构，再序列化；输出检查 UTF-8 字节数 |
| JSON 深度/节点 | 64 / 100000 | 输入和输出独立检查 |

CPU 执行通过 interrupt 检查期限、取消和关闭；Promise 等待同时有宿主 deadline 与取消监测。同步纯函数、简单 async/Promise 和静态模块初始化受到相同限额。取消不再消费无用计算；关闭拒绝新请求、结束活跃调用、拒绝队列剩余项并等待线程回收。

没有向 JS 注入文件、网络、环境变量、进程、Node、SQLite、任意 Rust 指针或数据库 Host API。当前只支持纯计算，不能用 `ctx.resources` 操作数据库。受限 QuickJS 不能替代操作系统隔离；这些限额用于可信项目的故障和资源控制。

## JSON 与错误契约

参数与返回值只接受 JSON 数据。所有整数必须在 ±9007199254740991 内，避免 Rust JSON 到 JavaScript 的静默舍入。非整数使用有限的 JavaScript Number；需要精确十进制时应由业务明确使用字符串协议。

输出检查器在用户模块加载前捕获原生方法，拒绝 undefined、函数、Symbol、BigInt、NaN/Infinity、不安全整数、循环、稀疏数组、访问器、特殊对象原型和额外数组属性。不会通过直接 JSON.stringify 把这些值静默丢失或转为 null。修改 JSON.stringify/Object 原型不能跳过验证。返回 Promise 先等待，再执行同一输出检查。

异常和无效结果由 Runtime 转成独立错误类型。HTTP 不输出用户异常文字、堆栈、源码路径或参数；日志只记登记 ID、耗时和结果类别。OOM 可以表现为引擎分配错误或被捕获的通用执行异常，两者均不携带用户数据；真实 2 MiB 内存压力测试同时验证随后正常调用仍可完成。

## 客户端模块与源码切片

独立 UI 模块通过 Vite/React 构建，保留 Hooks、Props、泛型、CSS 和用户选择的 React 组件库。运行引用保留原 import 门面和导出名；即使最终符号由 barrel 转导，也不会跳过门面 CSS/初始化。Schema 展开和静态求值仍追踪实际声明。深层 namespace 组件成员链目前明确要求改为直接导入。

应用入口及 `.diu` 文件不会作为浏览器入口整体执行。`ClientExpression`、声明入口中导出的普通组件/函数使用同一个 source_slice 收集器，按 KEngine 词法绑定追踪实际捕获；只选取所需导入、函数和逐项 const，保留源码顺序，剥离 TS 后保留 JSX 给前端工具。动态表达式注册为工厂，函数 Props 保留函数值，不直接 JSON 序列化。

切片目前不支持捕获可变 let/var、class、复杂解构、非模块局部环境或无法证明无副作用的初始化；诊断会建议拆成独立 UI 模块。此限制针对声明模块切片，独立普通 React 模块不因此失去完整 JS 语法。应用入口中未被捕获的普通副作用导入不会自动搬入切片；CSS 应由实际 UI 模块或其门面导入。

ServerFunction 根及运行期依赖默认私有；显式 SharedFunction 或 `.shared.` 可以标明纯共享依赖，但同一个模块一旦登记为 ServerFunction 根，就不能同时登记浏览器或 Shared 用途。客户端构建再次执行边界检查，不能仅依赖 tree-shaking 隐藏服务端代码。

## Action 调用和发布

`POST /api/actions/{name}` 接受唯一字段 `{"args":[...]}`，返回 `{"data":...,"generation":...}`。仅 Server/Shared Action 注册服务端路由。请求必须通过现有 Session、同源 Origin、CSRF、Execute 权限和代际检查；未知动作 404，未授权 401/403，不支持方法 405，参数错误 400，超时 504，繁忙/关闭 503，内部异常 500。调用按真实连接 IP 每分钟 60 次、Session 每分钟 20 次限流。

浏览器通过 `@kenvoy/client` 的 `callAction(name,args)` 或 `useAction(name)` 使用现有 API 客户端，不单独处理 Cookie，也不自动重试 Action。SDK 只提供受控调用，不包含服务端实现。

`JavascriptRuntime::with_bundle` 生成共享同一有界线程、持有不同不可变包的代际句柄。HTTP `publish_prepared` 在同一应用写锁中校验函数登记、执行允许的迁移，再同步发布 Application IR、assets、JS 和 generation；数据库提交后没有新的可失败异步步骤。旧请求继续使用旧包，发布者取消不撤销已接受任务。开发协调器及生产产物加载已消费该接口，流程见[运行时设计](runtime-design.md)。

证据见 [阶段 08 验证](verification/0010-typescript-extensions.md)，相关协议见 [Application IR](application-ir.md)、[安全](security-design.md)、[DSL](dsl-specification.md)。
