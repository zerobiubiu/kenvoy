# 验证记录索引

状态：现行。保存各次技术探针、编译、测试和验收的环境、命令、结果与限制。返回[文档总索引](../README.md)。

## 从哪里开始

- 核对第一版基础验收：先读[阶段 10 最终验收](0012-final-acceptance.md)，其中包含最终二进制、命令及能力映射。
- 核对 HMR、生产产物和真实 Ctrl+C：读[阶段 09 生命周期](0011-cli-lifecycle.md)。
- 复现检查：先读[测试策略](../testing-strategy.md)，再按下表进入相关阶段；现有实现和剩余风险从[实施路线](../development-roadmap.md)与[风险清单](../technical-risks.md)查询。

## 记录清单

下表按形成顺序保留原文件编号。结果属于各记录对应的修订和环境，不累加不同阶段的测试数量；后续总验收不改写早期失败或有限覆盖。

| 编号 | 文档 | 记录状态与范围 | 相关代码或配置 |
|---|---|---|---|
| 初始审查 | [阶段 00 首轮验证报告](../verification-report.md) | 参考；原始骨架的历史验证，不代表当前功能 | 原始双 Crate 骨架 |
| P01 计划 | [依赖与探针计划](p01-dependency-plan.md) | 参考；保留当时待批准的准备事实，执行结果见 0001 | Cargo.toml/Cargo.lock、KEngine 测试 |
| 0001 | [阶段 00 技术探针](0001-stage00-probes.md) | 已执行；最小可行性 | 两包探针、前端锁定资产 |
| 0002 | [Axum/React/Chromium 同源验证](0002-axum-react-browser.md) | 已执行；显式跨工具验证 | p03_http_sqlite.rs、axum-browser.mjs |
| 0003 | [阶段 01 KEngine 与退出保护](0003-kengine-foundation.md) | 已执行；解析基础、MSRV、资源故障分支 | kengine/src、engine_api、engine_boundary |
| 0004 | [阶段 02 通用语义与增量](0004-kengine-semantics.md) | 已执行；作用域、链接与增量 | KEngine 语义、表达式、会话缓存 |
| 0005 | [阶段 03 DSL 与基础 CLI](0005-dsl-cli.md) | 已执行；业务模型与基础命令 | Kenvoy DSL、字段验证、项目加载 |
| 0006 | [阶段 04 SQLite 与迁移](0006-sqlite-runtime.md) | 已执行；存储、迁移及后补压力测试 | database、migrate |
| 0007 | [阶段 05 HTTP 与认证](0007-http-auth.md) | 已执行；正式 HTTP、认证、权限 | HTTP、Auth、Session、admin CLI |
| 0008 | [阶段 06 React 表单](0008-react-forms.md) | 已执行；首次浏览器登记及后续定向修补 | React Runtime、共享校验、同源 API |
| 0009 | [阶段 07 管理后台](0009-admin-browser.md) | 已执行；该阶段同一构建的前后台闭环 | Admin、复用表单/表格、E2E |
| 0010 | [阶段 08 TypeScript 扩展](0010-typescript-extensions.md) | 已执行；组件、函数、隔离及后补 JS 定向验证 | 客户端构建、QuickJS、Action |
| 0011 | [阶段 09 CLI 生命周期](0011-cli-lifecycle.md) | 已执行；生产、HMR、失败恢复与真实退出 | 产物、开发协调、代理、资源关闭 |
| 0012 | [阶段 10 最终验收](0012-final-acceptance.md) | 已执行；第一版 Windows 基础验收 | 最终二进制、示例、完整闭环 |

## 关联实现与验证入口

语言测试位于 [KEngine tests](../../crates/kengine/tests/)，宿主集成测试位于 [Kenvoy tests](../../crates/kenvoy/tests/)，浏览器脚本位于 [frontend/scripts](../../crates/kenvoy/frontend/scripts/)。项目验证要求见 [AGENTS.md](../../AGENTS.md)；执行命令及前置依赖从[测试策略](../testing-strategy.md)进入。

## 维护、废弃与替代关系

准备计划不证明技术已可用，“已执行”仅表示所记录的验证已发生。定稿后保留命令、退出码、失败与边界，后续结果或勘误另建编号记录。0012 汇总最终验收，但不替代前阶段原始证据；当前没有正式废弃、替代或勘误条目。

初始报告继续保留在 `docs/verification-report.md`，不为目录形式迁移既有文件。
