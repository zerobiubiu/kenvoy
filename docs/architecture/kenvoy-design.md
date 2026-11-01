# Kenvoy 实现设计

状态：参考（现状文档）；正式解释器、存储、认证、HTTP、React 和 JavaScript 扩展已实现，生命周期按验证索引验收。

Kenvoy 是唯一用户 Binary，包含 CLI、项目加载、DSL 分析、业务模型、数据库、认证、HTTP、前端资产及 JavaScript 运行时。它通过 KEngine 公共 API 获得源码结构；TypeScript 去类型与代码生成属于 Kenvoy 自己的 JavaScript 适配模块，不调用 KEngine 私有实现。

基础设施中的 Project 从项目根解析严格类型化配置，保护源码、数据和输出目录边界。应用编译用例消费 AnalysisSession 的增量语言结果；DSL 静态求值和两遍资源分析构建版本化 ApplicationIR。真实 SWC 转换器通过静态端口注入，客户端切片仍在 check 阶段校验，不推迟到 build。业务错误保留来源与建议，不能用部分模型启动。

ApplicationIR 是业务单一来源。Resource 定义决定字段校验、SQLite 结构和统一 CRUD；Permission 按单项策略匹配，不能并集授权；Page 与 Component 转成受控客户端 DTO。源码位置、绝对模块路径和服务器实现不在客户端 DTO 中。

基础设施的 Database worker 专属线程持有 SQLite 连接，通过有界队列执行命令。应用层 Auth 依赖身份仓储和凭据端口，由 SQLite 与系统密码/时间适配器实现；认证表仍有独立版本和完整结构核验。资源用例统一执行授权及投影，并经仓储端口进入原有原子事务；HTTP handler 不直接操作数据库 actor、不执行同步 SQL，也不重新解析源码。

应用层 publication 持有代际快照及发布协调，读请求使用一致快照。发布操作持写锁并由受跟踪任务完成；数据库成功后同步替换运行模型，HTTP 只做传输适配。调用方取消不使已经接受的迁移停留在“数据库新、HTTP 旧”。bootstrap 的 Development 协调 notify、增量 AnalysisSession、构建候选与 Vite 镜像；基础设施的版本化产物支持没有源码和 Node 的生产运行。

产品技术适配均留在 Kenvoy 内部 module，不为适配器新增产品 Crate。Workspace 的测试和 xtask 成员只承担工程职责，见[工程与测试流程](../development/engineering-workflow.md)。当前没有多数据库、多前端框架或多 HTTP 引擎实现，避免用空抽象伪装可替换能力。

相关规范与契约：[DDD 目录与依赖](ddd-structure.md)、[Application IR](../contracts/application-ir.md)、[解释器](interpreter-design.md)、[运行时](runtime-design.md)、[安全](security-design.md)、[实施路线](../development/development-roadmap.md)。
