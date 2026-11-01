# KEngine 实现设计

状态：现行；通用解析、语义、模块绑定与增量会话已验收。

KEngine 是 Library，入口集中于 Engine 和 AnalysisSession。Parser 使用 SWC，AST 仅作为内部实现；公共 Generic IR、诊断、符号和模块图不引用 Kenvoy 类型。解析 JSX 标签时不判断其是否为 HTML、React 组件或业务声明，连小写、下划线和美元符号开头的组件引用也按语言词法处理。

解析层保留原始源码、UTF-16 字符串值、表达式结构、源码范围及无法静态求值的引用。表达式结构不会在 KEngine 内执行。变量声明保留单独的绑定模式与 initializer，调用方无需扫描所有嵌套 JSX 猜测默认导出。

语义层以模块内 ID 区分 Scope、Symbol 和 Reference。函数参数环境与函数体分开，避免默认参数错误绑定函数体的 var。值与类型命名空间分开，typeof 的类型查询可引用仅类型导入的值视图，不能因此重新开放运行时导出。

模块链接通过固定点计算处理循环、export star 与显式覆盖；歧义和缺失导出有诊断。External 仅表示调用方明确允许的外部引用，不证明 npm 包已安装或导出正确。

AnalysisSession 拥有缓存，按规范路径、源码内容和解析选项复用解析结果。每次更新重新核对模块解析和绑定，变化模块与前后图的反向依赖决定受影响集合。错误候选不能替换最后一次有效语言分析；Kenvoy 还须单独维护最后有效业务模型。

完整公开接口见 [KEngine API](kengine-api.md)，结构与兼容规则见 [Generic IR](generic-ir.md)。测试分布于 kengine/tests；[阶段 02 验证](verification/0004-kengine-semantics.md)记录 66 项 KEngine 测试与 MSRV 的实际时间边界。当前没有完整 TypeScript 类型检查器、npm exports 解析器、FFI 或 WASM 绑定。
