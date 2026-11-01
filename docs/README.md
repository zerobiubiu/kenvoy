# Kenvoy 文档导航

状态：现行；核对日期：2026-11-01。

这里是详细项目文档的统一入口。主题文档说明当前契约，ADR 说明决策原因，验证与变更记录保留各阶段事实。项目执行规则见 [AGENTS.md](../AGENTS.md)。

## 从这里开始

| 目的 | 推荐阅读顺序 |
|---|---|
| 创建并运行应用 | [安装与运行](../README.md) → [示例项目](../examples/README.md) → [CLI](cli-design.md) → [配置](project-configuration.md) |
| 编写声明与扩展 | [DSL 语法](dsl-specification.md) → [前端 Runtime](frontend-runtime.md) → [JavaScript Runtime](javascript-runtime.md) → [扩展示例](../examples/extensions-app/README.md) |
| 了解整体设计 | [架构](architecture.md) → [Crate 边界](crate-boundaries.md) → [技术栈](technology-stack.md) → 下方对应模块 |
| 确认完成范围与待办 | [阶段状态与后续工作](development-roadmap.md) → [最终验收](verification/0012-final-acceptance.md) → [风险与未覆盖项](technical-risks.md) |
| 参与开发与验证 | [测试策略](testing-strategy.md) → [验证记录索引](verification/README.md) → [变更记录索引](changes/README.md) |

## 主题文档

沿用项目既有根目录文件名和路径，在本索引中按主题分组；历史引用继续有效。

### 整体架构与技术选择

| 文档 | 阅读范围 |
|---|---|
| [架构总览](architecture.md) | 两层模型、组件关系、生命周期与历史基线 |
| [Crate 职责边界](crate-boundaries.md) | 两个成员、单向依赖和不可跨越的责任 |
| [技术栈](technology-stack.md) | 当前依赖、来源、实测兼容性和平台边界 |
| [技术决策汇总](technical-decisions.md) | 当前选择及代价；历史原因由 ADR 展开 |
| [阶段状态与后续工作](development-roadmap.md) | 阶段 00—10 状态、首版范围及后续方向 |
| [技术风险](technical-risks.md) | 已有证据、未实现项与未覆盖验证场景 |

### KEngine：通用语言引擎

相关实现：`crates/kengine/src/`、`crates/kengine/tests/`。

| 文档 | 阅读范围 |
|---|---|
| [KEngine 设计](kengine-design.md) | Parser、语义、链接和增量模块分工 |
| [编译管线](compiler-design.md) | 从源文件到通用分析结果及调用方的完整流程 |
| [公共 API](kengine-api.md) | 调用接口、结果所有权、诊断与支持边界 |
| [Generic IR](generic-ir.md) | 通用节点、表达式、模块、符号及版本协议 |

### Kenvoy：声明与业务模型

相关实现：`crates/kenvoy/src/dsl/`、`application.rs`。

| 文档 | 阅读范围 |
|---|---|
| [Kenvoy 设计](kenvoy-design.md) | 项目加载、业务分析和宿主模块分工 |
| [DSL 设计](dsl-design.md) | 语义取舍、命名、权限与组件分类原则 |
| [DSL 语法规范](dsl-specification.md) | 实际支持的声明、属性和静态分析限制 |
| [Application IR](application-ir.md) | 宿主业务模型、客户端投影与注册标识 |
| [解释器分工](interpreter-design.md) | Page、Resource、API、Admin、Action 的执行责任 |

### 运行、存储与安全

相关实现：`crates/kenvoy/src/`、`crates/kenvoy/frontend/`。

| 文档 | 阅读范围 |
|---|---|
| [运行时与生命周期](runtime-design.md) | 启动、候选发布、HMR、生产产物和资源关闭 |
| [CLI](cli-design.md) | 用户命令、参数、输出及失败行为 |
| [项目配置](project-configuration.md) | 默认值、路径规则、重载与部署配置 |
| [前端 Runtime](frontend-runtime.md) | 描述解释、表单/后台、组件构建和更新 |
| [JavaScript Runtime](javascript-runtime.md) | TS 转换、模块、Action、限额及隔离 |
| [数据库设计](database-design.md) | 连接所有权、字段映射、Schema 和数据一致性 |
| [迁移执行协议](migration-design.md) | 预览、确认、备份、执行与失败处理 |
| [安全设计](security-design.md) | 认证、授权、会话、请求和开发代理边界 |
| [测试策略](testing-strategy.md) | 验证入口、测试分层和证据要求 |

前端产品与视觉约定另见仓库根目录的 [PRODUCT.md](../PRODUCT.md) 和 [DESIGN.md](../DESIGN.md)。

## 决策、验证与变更历史

| 分类 | 入口 | 如何使用 |
|---|---|---|
| 决策 | [adr/README.md](adr/README.md) | 了解为什么采用某项方案；当前适用性由索引说明 |
| 验证 | [verification/README.md](verification/README.md) | 查实际命令、环境、结果和限制；最新综合证据为[阶段 10](verification/0012-final-acceptance.md) |
| 变更 | [changes/README.md](changes/README.md) | 查各次实现范围及对应验证，按原编号追踪 |
| 初始审查 | [阶段 00 骨架报告](verification-report.md) | 只描述初始骨架及当时未实现项，不代表当前状态 |

## 文档维护约定

- 根目录主题文档持续维护当前有效契约，未来方案与未支持能力明确标注；精确测试结果保留在验证记录中。
- 主题职责以本索引为准：设计解释原因，规范/API 解释契约，运行时解释执行过程，历史记录解释某次实施与验证。交叉内容使用链接，避免复制多个会独立过期的完整清单。
- 定稿 ADR、验证与变更记录不改写、不重排编号；后续变化或勘误另建记录，适用性和替代关系在分类索引维护。
- 新增文档同步更新本索引及对应分类入口；分类有实际内容时才建立目录与 README，不创建空分类。
- 纯文档整理不递归产生变更记录。现状文档使用“现行/参考/已过期/已废弃”，执行记录使用“待执行/已执行/已过期/已废弃”；“已执行”不代表方案永久有效。
- 文档校验入口为 `node tools/verify-docs.mjs`，检查 UTF-8、本地链接、空白及从本索引的可达性。项目规则以 [AGENTS.md](../AGENTS.md) 为准。

当前没有废弃或替代的文档文件；旧 XML/SSR/PostgreSQL 路线未作为当前设计恢复。
