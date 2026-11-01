# 阶段状态与后续工作

状态：现行；核对日期：2026-11-01。

第一版基础框架的阶段 00—10 已完成，适用范围是验收记录中的 Windows 环境和实际用例。完整功能闭环通过，不等于跨平台发行、全部故障场景或所有可选扩展均已完成。精确命令和测试计数统一见[最终验收](verification/0012-final-acceptance.md)。

## 已完成阶段

| 阶段 | 已完成范围 | 主要证据 |
|---|---|---|
| 00 审查与技术验证 | 双 Crate 边界、架构、ADR、SWC/JS/SQLite/HTTP/React/监听探针 | [初始审查](verification-report.md)、[技术探针](verification/0001-stage00-probes.md)、[同源浏览器补验](verification/0002-axum-react-browser.md) |
| 01 KEngine 基础 | 真实 JS/TS/JSX/TSX、通用 IR、模块结构与定位诊断 | [阶段 01](verification/0003-kengine-foundation.md) |
| 02 通用分析 | 作用域、符号、重导出/循环、结构化表达式和增量会话 | [阶段 02](verification/0004-kengine-semantics.md) |
| 03 业务 DSL | 静态声明、资源推导、Application IR、冲突与边界检查 | [阶段 03](verification/0005-dsl-cli.md) |
| 04 SQLite | 专属连接线程、真实 CRUD、Schema Diff、迁移/备份/回滚 | [阶段 04](verification/0006-sqlite-runtime.md) |
| 05 Rust HTTP | Axum、认证、Session、权限、CSRF 与通用 API | [阶段 05](verification/0007-http-auth.md) |
| 06 React Runtime | 描述解释、字段、共享校验、真实登记与交互反馈 | [阶段 06](verification/0008-react-forms.md) |
| 07 管理后台 | 登录、列表/搜索/筛选/排序/分页、详情及增改删 | [阶段 07](verification/0009-admin-browser.md) |
| 08 TypeScript 扩展 | 普通 React、Schema 组合、受限 TS/ESM/Promise Action | [阶段 08](verification/0010-typescript-extensions.md) |
| 09 CLI 生命周期 | 七个命令、产物、自动生产构建、监听/HMR、错误恢复与真实 Ctrl+C | [阶段 09](verification/0011-cli-lifecycle.md) |
| 10 综合验收 | 最终二进制上的浏览器、API、持久化、生产与扩展闭环 | [阶段 10](verification/0012-final-acceptance.md) |

阶段 06 的前端入口随后在阶段 07—10 完善并统一复验；不能将早期记录的有限覆盖或当时计数直接作为最终结果。历史记录原文保留在[验证索引](verification/README.md)，实施文件范围见[变更索引](changes/README.md)。

## 第一版完成范围

客户应用已通过 CLI 创建并启动，页面提交真实写入 SQLite，管理员能够查询与管理数据；重新启动后记录保留。dev 与 run 使用相同资源、权限和数据库路径。run 不要求预先 build，也可加载不含源码的完整产物。

扩展应用已验证普通 React Hooks/Props/CSS、第三方组件、跨文件 Schema 与服务端纯计算函数。开发更新验证了兼容组件的状态保持、错误候选保留、安全增字段与危险迁移阻断；基础和自定义应用另经真实终端 Ctrl+C 验证。

上述能力与验收用例的完整映射见[最终报告](verification/0012-final-acceptance.md)。只使用内置 DSL 的运行步骤见[项目 README](../README.md)，支持的语言/DSL 边界分别见[KEngine API](kengine-api.md)与[DSL 语法](dsl-specification.md)。

## 后续工作分组

以下是未完成范围，不表示已排定版本、日期或新的执行授权。

| 分组 | 当前边界 | 后续方向 |
|---|---|---|
| 工程验证 | Windows 基础场景已验收；跨平台、长期负载及部分故障路径未覆盖 | 按部署目标补充平台矩阵、磁盘满/断电/网络盘、构建或发布中途退出验证 |
| 可选业务扩展 | 重命名只检测并阻断；未提供上传/关系字段和数据库/文件/网络 Host API | 分别设计显式迁移映射及受控扩展，复用现有权限和数据保护规则 |
| 执行环境扩展 | QuickJS 仅执行可信项目代码，不可信输入明确拒绝 | 需要不可信执行时，单独设计并验证进程或操作系统隔离 |
| 发布与运维 | 尚无正式发行、签名或发布流程；历史构建版本不自动回收 | 明确版本来源、供应链/许可证核对、分发方式及产物保留策略 |

完整 TypeScript 类型检查、任意动态 Schema、完整 Node/Bun 兼容和多框架适配不是首版承诺。各项剩余风险、现有证据和具体限制集中维护在[技术风险](technical-risks.md)，不在本页重复其详细清单。

## 后续实施与文档维护

后续仍按审查、设计、实施、验证、验收和文档同步推进；前一阶段核心验证失败时先修复。双 Crate、数据保护、依赖授权、Git 与版本规则以 [AGENTS.md](../AGENTS.md) 为准，不因首版完成而放宽。

源码变更更新对应现状文档并新增实施记录；重大路线变化新增 ADR，历史正文不改写。验证方法和环境要求见[测试策略](testing-strategy.md)，命令只能按实际执行结果记录。
