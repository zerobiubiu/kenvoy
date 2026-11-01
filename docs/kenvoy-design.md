# Kenvoy 实现设计

状态：现行；正式解释器、存储、认证、HTTP、React 和 JavaScript 扩展已实现，生命周期按验证索引验收。

Kenvoy 是唯一用户 Binary，包含 CLI、项目加载、DSL 分析、业务模型、数据库、认证、HTTP、前端资产及 JavaScript 运行时。它通过 KEngine 公共 API 获得源码结构；TypeScript 去类型与代码生成属于 Kenvoy 自己的 JavaScript 适配模块，不调用 KEngine 私有实现。

Project 从项目根解析严格类型化配置，保护源码、数据和输出目录边界。AnalysisSession 提供增量语言结果；DSL 静态求值和两遍资源分析构建版本化 ApplicationIR。业务错误保留来源与建议，不能用部分模型启动。

ApplicationIR 是业务单一来源。Resource 定义决定字段校验、SQLite 结构和统一 CRUD；Permission 按单项策略匹配，不能并集授权；Page 与 Component 转成受控客户端 DTO。源码位置、绝对模块路径和服务器实现不在客户端 DTO 中。

Database worker 专属线程持有 SQLite 连接，通过有界队列执行命令。Auth 使用同一 worker 的固定认证命令，但认证表有独立版本和完整结构核验；业务 Resource API 不能访问内部表。HTTP handler 不执行同步 SQL，也不重新解析源码。

HttpRuntime 持有应用代际及资源目录，读请求使用一致快照。发布操作持写锁并由受跟踪任务完成；数据库成功后同步替换 HTTP 模型。调用方取消不使已经接受的迁移停留在“数据库新、HTTP 旧”。Development 协调 notify、增量 AnalysisSession、构建候选与 Vite 镜像，版本化产物支持没有源码和 Node 的生产运行。

技术适配均留在 Kenvoy 内部 module，不新增 Crate。当前没有多数据库、多前端框架或多 HTTP 引擎实现，避免用空抽象伪装可替换能力。

相关契约：[Application IR](application-ir.md)、[解释器](interpreter-design.md)、[运行时](runtime-design.md)、[安全](security-design.md)、[实施路线](development-roadmap.md)。
