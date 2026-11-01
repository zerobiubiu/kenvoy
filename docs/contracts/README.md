# 接口与数据契约

状态：参考；治理：现状文档。

[返回文档中心](../README.md)

## 目录职责

本目录集中维护 KEngine 公共 API、Generic IR、Kenvoy DSL、Application IR 与客户端投影、项目配置，以及组件绑定和客户端 SDK 的使用契约，明确输入输出、格式版本、约束和边界。

设计动机、模块所有权和运行流程见[架构与模块设计](../architecture/README.md)；本目录通过链接衔接设计说明，不复制完整设计文档。Generic IR 与 Application IR 分属语言层和业务层，分别版本化，不把内部 SWC 类型、完整宿主 IR 或服务端实现当作浏览器协议。命令及迁移操作见[运维入口](../operations/README.md)，历史决策见[决策索引](../decisions/README.md)，实际验收范围见[验证索引](../verification/README.md)。

## 文档清单

以下共 6 篇主题文档，均为可持续维护的参考现状文档。清单说明契约覆盖面，不额外宣称实现或验收通过；已实现、受限及待验证行为以正文、当前实现和原始证据为准。

| 文档 | 状态 | 内容概览 |
|---|---|---|
| [KEngine 公共 API](kengine-api.md) | 参考 | 解析、项目分析、模块解析和增量会话的参数、返回值、结果所有权、诊断及错误边界。 |
| [Generic IR v1](generic-ir.md) | 参考 | 通用源码、JSX、表达式、声明、语义、模块绑定和图的结构、位置及序列化兼容规则。 |
| [已实现声明语法](dsl-specification.md) | 参考 | DSL 根结构、标签与字段、权限、Action、静态求值、模块边界和数据输入约束。 |
| [Application IR 与客户端描述](application-ir.md) | 参考 | 统一业务模型、对象标识、模块引用、身份投影及运行代际与产物的衔接。 |
| [项目配置与路径规则](project-configuration.md) | 参考 | `kenvoy.toml` 默认值、严格校验、路径保护、开发重载和生产部署输入。 |
| [组件重载、数据绑定与联动选项](component-bindings.md) | 参考 | 系统组件重载、字段及表单绑定、静态与跨资源选项接口、权限和 `@kenvoy/client` SDK。 |

## 推荐阅读顺序

1. 首先阅读[职责边界](../architecture/crate-boundaries.md)，区分 KEngine 的语言分析和 Kenvoy 的业务解释，避免混用两层模型。
2. 接入 KEngine 时按 [API](kengine-api.md) → [Generic IR](generic-ir.md)阅读；结果所有权和调用方式对应 [KEngine 设计](../architecture/kengine-design.md)，解析与缓存流程对应[编译管线](../architecture/compiler-design.md)。
3. 编写应用时先读[项目配置](project-configuration.md)，再读 [DSL 规范](dsl-specification.md)；设计原则见 [DSL 设计](../architecture/dsl-design.md)，业务对象和客户端格式查 [Application IR](application-ir.md)。
4. 扩展 UI 或访问资源时读[组件绑定与 SDK](component-bindings.md)，结合[前端设计](../architecture/frontend-runtime.md)、[JavaScript Runtime](../architecture/javascript-runtime.md)和[安全设计](../architecture/security-design.md)核对客户端、服务端及权限边界。
5. 涉及启动、更新或迁移时，联读[运行时设计](../architecture/runtime-design.md)、[数据库设计](../architecture/database-design.md)及[运维入口](../operations/README.md)；判断能力是否已验证时只引用[验证索引](../verification/README.md)对应记录，不把计划或历史阶段结果视为新接口的验收。

## 相关代码、配置与示例

| 契约关注点 | 当前实现或使用入口 |
|---|---|
| KEngine API | [公共导出](../../crates/kengine/src/lib.rs)、[API 消费测试](../../crates/kengine/tests/engine_api.rs)、[Kenvoy 边界消费测试](../../crates/kenvoy/tests/engine_boundary.rs) |
| Generic IR | [IR 类型与序列化](../../crates/kengine/src/ir.rs) |
| DSL | [分析器](../../crates/kenvoy/src/dsl/analyzer.rs)、[静态求值器](../../crates/kenvoy/src/dsl/static_eval.rs)、[客户应用声明示例](../../examples/customer-app/src/app.tsx) |
| Application IR 与浏览器投影 | [宿主模型](../../crates/kenvoy/src/application.rs)、[HTTP 投影](../../crates/kenvoy/src/http/projection.rs)、[客户端类型](../../crates/kenvoy/frontend/src/types.ts) |
| 配置 | [项目加载与校验](../../crates/kenvoy/src/project.rs)、[客户应用配置](../../examples/customer-app/kenvoy.toml) |
| 组件绑定、SDK 与选项 | [绑定类型](../../crates/kenvoy/frontend/src/bindings.ts)、[SDK 入口](../../crates/kenvoy/frontend/src/sdk.ts)、[客户端 API](../../crates/kenvoy/frontend/src/api.ts)、[服务端选项规则](../../crates/kenvoy/src/options.rs)、[组件扩展示例](../../examples/extensions-app/README.md) |

上述测试链接用于定位既有消费方式，不表示本次目录整理重新运行了这些测试。

## 维护与废弃、替代关系

- 本目录各主题继续维护，目录迁移不构成接口废弃。公开参数、DSL、模型、默认值或 SDK 行为发生变化时，更新对应契约、相关设计和使用示例，并同步本清单；不在多个入口各维护一份完整规范。
- Generic IR、Application IR 和客户端投影的版本边界分别遵守对应正文；不能用 Cargo 或 SWC 版本变化代替协议兼容说明，也不能把旧快照缺失的新语义视为已完成分析。破坏性变更须明确拒绝或迁移方式，并链接关联决策和记录。
- 组件重载和 SDK 复用原生资源、字段、验证与权限，不另建一套业务 Schema。默认前端不再内置 MUI/Emotion；迁移前证据不自动覆盖[组件与联动选项](component-bindings.md)中的新增契约，具体变化及证据由该篇所链接的记录承载。
- 历史 ADR、变更和验证正文不可变，不合并、不删除、不重编号；后续替代或勘误另建记录，通过[决策索引](../decisions/README.md)、[变更索引](../changes/README.md)和[验证索引](../verification/README.md)说明关系。现行契约不是未来计划，设计意图和后续计划也不等于验收完成；本入口不生成新的测试结论。
