# KEngine 通用语义与增量分析

状态：已执行；日期：2026-11-01；前置：[解析基础](0002-kengine-foundation.md)。

## 背景与方案

原有 JSX/表达式摘要不能区分直接导出结构和函数体 JSX，也缺少作用域及跨模块实体定位。为后续宿主 DSL 消费新增通用结构和符号绑定，仍不赋予任何 JSX 标签业务语义。保留无状态 Engine，增量缓存单独由 AnalysisSession 管理。

## 文件与接口

- 新增 semantic.rs、linker.rs、incremental.rs；修改 parser.rs、ir.rs、engine.rs、lib.rs 和 KEngine Cargo 依赖。
- 新增 expression_structure、semantic_api、semantic_regressions、module_bindings、linker_regressions、incremental 六组测试。
- 新增 tools/verify-architecture.mjs、tools/verify-docs.mjs，验证实际 Cargo 边界与文档链接/编码。
- 更新 KEngine API、Generic IR、编译设计、路线与文档索引；新增 Application IR 的阶段 03 设计约束。
- 已锁定 swc_ecma_transforms_base 51.0.0 提升为 KEngine 普通依赖，用于词法 resolver；未引入新的依赖版本。

新增 API 包含 ExpressionShape、VariableDeclaratorIR/BindingPatternIR、SemanticIR、ModuleBindingsIR、AnalysisSession/Update/Stats。v1 新字段有保守 serde 默认值，旧快照不假装具有完整结构或绑定。无 Git 提交、版本递增、数据迁移或用户数据删除。

## 执行验证

Windows MSVC、Rust 1.99.1。workspace check/test/Clippy、KEngine 单独 check 及 fmt 最终均退出 0；81 项常规测试通过，1 项原有浏览器探针忽略。独立审查发现并修复 JSX 名称/参数作用域、类型查询、转导歧义和非法名称问题。详细命令、MSRV 范围和实际失败修复见 [阶段 02 验证](../verification/0004-kengine-semantics.md)。

下一阶段只在 Kenvoy 中实现静态 DSL 与 Application IR，继续验证未知动态声明拒绝、UI/Schema 分离、资源推导、路由冲突和默认权限；不能从本阶段通用能力推断业务已经完成。
