# 当前技术决策与依据

状态：现行；历史决策正文保留在 [ADR 索引](adr/README.md)，后续重大路线变化新增 ADR。

| 决策 | 采用原因 | 已知代价与验证边界 |
|---|---|---|
| 两个 Rust Crate | 通用语言引擎与应用宿主单向隔离 | Kenvoy 内模块较多，不能以拆包规避边界 |
| SWC → Generic IR | 复用真实语言解析，公共契约不绑定 SWC AST | 保真转换和作用域/导出语义需要专门回归 |
| Application IR 统一解释 | 页面、权限、API 与 Schema 共用模型 | 候选发布必须保证代际一致 |
| 专属 SQLite worker | Connection 生命周期与线程所有权明确 | 读写初版串行，有界队列过载拒绝 |
| SQLite 自定义迁移 | 能预览并保护已有数据，符合原生宿主 | 不承诺所有改名/类型转换自动完成 |
| Axum/Tokio | HTTP 与静态资源由 Rust 宿主控制 | 慢请求、退出和发布任务需明确界限 |
| React/TypeScript/原生 CSS/CSR | 通用组件树支持表单、后台与普通 React 扩展 | 前端源码仍需构建，不能直接序列化函数 |
| Vite 8 + Rolldown + React 插件 | 受控构建与 React Refresh，Axum 提供同源入口 | 仅开发模式启动内部 Node 子进程；真实完整热更新见阶段 09 验证 |
| Argon2id + 服务端 Session | 密码独立盐，Cookie 仅保存随机凭据 | 同源/CSRF/限流不可由 SameSite 替代，暂无 MFA |
| QuickJS/rquickjs 执行器 | 正式 Runtime 验证同步、Promise、ESM 与限额 | 不能兼容完整 Node；不可信项目明确拒绝，限额不等于 OS 沙箱 |
| 不可变版本目录与原子 manifest | 构建不清空用户目录，失败候选不激活 | 暂不自动清理历史版本，需由运维按保留策略管理磁盘 |

具体依赖和来源由根 Cargo.toml、Cargo.lock、框架前端 package.json/package-lock.json 锁定。新增依赖在用户已授权的工程范围内执行并记录；用户项目依赖不因框架启动而被静默改写。

前端默认发行资产嵌入 Kenvoy，使基础 DSL 应用运行不需要 Node。编译框架源码的人需先构建前端，构建脚本不会自动安装工具。自定义组件构建使用项目内独立的锁定工具缓存；依赖安装要求显式 --install-deps，package.json 不被静默改写。生产只需 Binary、配置、产物与独立 SQLite 目录。
