# 阶段 08：自定义组件、TypeScript 与受限 JavaScript

状态：已执行；日期：2026-11-01；环境：Windows、Rust 1.99.1、Node 24.22.0、Chromium 156.0.8078.4。基础实现与后续单项补测分别记录，不能将不同修订的计数合并为一次全量门禁。

## 验收范围

通用 React Runtime 现在能够加载用户独立组件及声明入口的受控切片。真实浏览器使用 React Hooks、自定义 Props、TypeScript 泛型转换、MUI、clsx、barrel CSS 和 `.diu.tsx` Schema Component。入口内箭头事件捕获 `step=2`，点击后计数为 2；随后通过公开 SDK 调用服务端折扣函数，Rust 宿主完成 TS→JS、跨模块 ESM、Promise 等待并返回 0.8。最终表单提交真实写入 SQLite。

JavaScript 正式模块基于阶段 00 探针使用同一 SWC/rquickjs 路线，新增版本化模块包、受控加载器、有界工作线程、每次调用独立上下文、JSON 检查及取消/关闭。HTTP Action 复用 Session、CSRF、Origin、代际和 Execute 权限。

## 统一验证

下列统一门禁由主执行代理运行并观察结果，本记录据其共享执行结果整理；不是把其他代理的推断写成执行证据。

| 命令 | 结果 | 退出码 |
|---|---|---|
| `cargo test --workspace --offline --locked` | 196 项常规测试通过，2 项需额外 Node/npm 或浏览器条件的显式测试保留 ignored | 0 |
| `npm test --prefix crates/kenvoy/frontend` | 86 项客户端测试通过 | 0 |
| `node crates/kenvoy/frontend/scripts/extensions-e2e.mjs` | Hooks/Props/inline capture、MUI/clsx/barrel CSS、Schema Component、服务端 TS/ESM/Promise、私有代码边界、用户 package.json 保持均通过 | 0 |

扩展 E2E 使用真实 CLI 临时项目，显式 `dev --install-deps` 安装锁定项目依赖；package.json 安装前后逐字一致。脚本检查产物 `modules/server.json` 包含专用私有标记，而所有登记 `client/` 文件均不包含；HTTP `/modules/server.json` 返回 404。业务函数抛出含私有标记的异常时 API 返回 500，响应中无该标记；浏览器 pageerror 为空。

## 定向验证与后补回归

| 命令或动作 | 结果 | 退出码 |
|---|---|---|
| `cargo test -p kenvoy --bin kenvoy dsl::static_eval::tests --offline --locked` | 13 项通过，包括运行引用保留 import 门面、别名与 namespace 一级成员，Schema 仍追踪实际声明 | 0 |
| `cargo test -p kenvoy --bin kenvoy javascript::compiler::tests --offline --locked` | 8 项通过：TS 消除、静态 ESM/转导、循环、type-only、能力限制、产物重新校验、普通 .ts 泛型及客户端 JSX 保留 | 0 |
| `cargo test -p kenvoy --bin kenvoy dsl::analyzer::tests --offline --locked` | 原有 11 项语义及边界测试通过 | 0 |
| `cargo test -p kenvoy --bin kenvoy server_actions_execute_compiled_esm --offline --locked` | 真实编译函数调用、匿名/admin Execute、CSRF、嵌套 JSON 查重、整数精度、500 脱敏、504、405、失败发布及新旧代际一致性通过 | 0 |
| `cargo test -p kenvoy --bin kenvoy javascript:: --offline --locked` | 最后补测后 18 项通过：compiler 8、runtime 6、source_slice 4；此时新增泛型切片/生成名碰撞测试不冒称已包含在前述 196 项全量中 | 0 |
| `cargo clippy -p kenvoy --bin kenvoy --offline --locked` | 退出 0；保留 5 组阶段 09 尚待消费的 dead_code 警告，未用 allow 隐藏 | 0 |
| 对 javascript、http、dsl/analyzer.rs 执行 `rustfmt --edition 2024` | 实际格式化通过，不改写无关项目文件 | 0 |

6 项 Runtime 测试包含同步/async JSON、跨调用全局隔离、缺少宿主能力、参数数量/大小/深度/安全整数、返回值禁止静默丢失、修改 JSON.stringify/Object 原型仍不能绕过验证、异常恢复和不可变函数包代际。queue=1 时第三次调用立即 Busy；取消前两次后正常调用恢复；关闭永不完成 Promise 在 500ms 测试期限内完成。

CPU 无限循环、无限 Promise 微任务和实际 2 MiB OOM 放入独立测试子进程，父进程有 8 秒 watchdog。CPU/微任务均超时，内存压力返回分配或通用执行异常而非超时，随后正常函数仍返回 42；子进程实际退出成功。该实验验证配置的 QuickJS 限额，不验证操作系统级恶意代码隔离。

4 项源码切片测试验证：入口同时注册 Server Action 时，纯客户端箭头只含捕获常量；局部泛型组件仅引入所需具名导入和单个 const；捕获私有模块与 Server/Shared 同模块注册均阻断；普通 .ts 泛型箭头不误当 TSX，生成导出名不覆盖用户符号。

## 失败与修正

开发期编译曾因新模块/辅助函数尚未落地失败，按跨代理文件接线顺序补齐后再验证，没有关闭检查。JsDiagnostic 的 `source` 是源码位置而非 std::error::Error，改用手工 Display/Error 保留结构化位置信息。

Runtime 初次 5 项中 1 项因测试直接写 `typeof require` 被已实现的保守标识符规则拒绝；测试改为检查全局对象对应属性是否不存在，最终全部通过。HTTP Action 首次夹具从已有 customers 资源切换到无资源应用，被删除迁移确认正确阻断；改为初始无资源页面夹具，验证函数发布本身而不绕过迁移规则，随后通过。一次测试使用了不存在的 Database.close，改为真实 shutdown API。

并行 Cargo 检查出现 Windows incremental 目录 `拒绝访问 (os error 5)` 提示；随后命令仍退出 0，提示说明该次缓存不能复用，不解释为功能错误被忽略。普通 Clippy 的未消费接口属于阶段 09 接入范围，当前不声称严格 `-D warnings` 无警告。

## 已知限制

只执行可信项目的本地静态 ESM 纯计算；无 npm/Node 兼容层、数据库/文件/网络 Host API，不可信模式明确拒绝。切片不支持复杂可变捕获、class、解构或未知初始化副作用，需拆成独立 UI 模块。Action 不自动重试，不支持流式结果。Windows 浏览器夹具停止服务采用强制终止，不作为 Ctrl+C 优雅退出证据；真实 HTTP 优雅关闭已有先前 Rust 测试，本阶段未借此声称 dev HMR、完整 run 或阶段 10 全部通过。

相关实现和契约见 [JavaScript Runtime](../javascript-runtime.md)、[Application IR](../application-ir.md)、[安全](../security-design.md)，实施范围见 [阶段 08 变更](../changes/0009-typescript-extensions.md)。后续需独立验收自动构建复用、受控热更新、错误恢复、生产启动与持久化。
