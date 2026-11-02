# 验证记录索引

状态：参考（现状文档）。保存各次技术探针、编译、测试和验收的环境、命令、结果与限制。返回[文档总索引](../README.md)。

## 从哪里开始

- 核对第一版基础验收：先读[阶段 10 最终验收](0012-final-acceptance.md)，其中包含最终二进制、命令及能力映射。
- 核对首版后的新增能力和工程入口：读下方[专项验证](#首版后的专项验证)，证据随对应变更记录保存，不将阶段 10 验收扩大为后续修订的验收。
- 核对 HMR、生产产物和真实 Ctrl+C：读[阶段 09 生命周期](0011-cli-lifecycle.md)。
- 复现检查：先读[测试策略](../development/testing-strategy.md)，再按下表进入相关阶段；现有实现和剩余风险从[实施路线](../development/development-roadmap.md)与[风险清单](../development/technical-risks.md)查询。

## 首版后的专项验证

以下记录位于 `changes/`，其编号不属于本目录的验证序列。这里只链接原始证据，不复制命令输出，也不累加不同修订的测试数量。

| 证据位置 | 记录状态与范围 | 对应现状文档 |
|---|---|---|
| [changes/0012：原生组件与联动选项](../changes/0012-native-components-and-options.md#执行验证记录) | 已执行；默认原生 CSS、系统组件重载、统一数据绑定、联动与跨资源选项 | [组件与选项契约](../contracts/component-bindings.md)、[前端 Runtime](../architecture/frontend-runtime.md) |
| [changes/0013：Rust 测试成员与工程入口](../changes/0013-rust-test-workspace-and-tools-retirement.md#执行验证记录) | 已执行；跨 Crate 测试、xtask、Rust 浏览器夹具及 tools 退役 | [工程与测试流程](../development/engineering-workflow.md)、[测试策略](../development/testing-strategy.md) |
| [changes/0014：多文件上传与图片](../changes/0014-file-uploads-and-images.md#执行验证记录) | 已执行；附件权限/生命周期、Image/SDK、dev→run 同库读写和桌面/390px 专项 | [文件与图片契约](../contracts/file-uploads.md)、[数据库设计](../architecture/database-design.md)、[后续待办](../development/implementation-backlog.md) |
| [changes/0015：上传与图片示例](../changes/0015-uploads-example.md#执行验证记录) | 已执行；正式示例副本的上传/下载、权限、URL/SDK 图片、生产重启、空状态与桌面/390px | [示例 README](../../examples/uploads-app/README.md)、[文件与图片契约](../contracts/file-uploads.md) |
| [changes/0016：DDD 分层与模块拆分](../changes/0016-ddd-structure-and-module-split.md#执行验证记录) | 已执行；AST 层依赖/规模门禁、Rust/前端/七套浏览器、CLI 逐字基线与旧库/旧附件读回 | [DDD 目录规范](../architecture/ddd-structure.md)、[工程流程](../development/engineering-workflow.md)、[ADR 0012](../decisions/0012-ddd-layers-and-module-boundaries.md) |
| [changes/0017：Windows 密码提示乱码修复](../changes/0017-windows-password-prompt-encoding.md#执行验证记录) | 已执行；代码页 936/65001 原生控制台、中文提示、不回显、输入模式恢复及错误拒绝 | [CLI 账号管理](../operations/cli-design.md#账号管理)、[密码约束](../architecture/security-design.md#账户初始化与密码) |
| [changes/0018：Runtime 根入口修复](../changes/0018-runtime-root-entry.md#执行验证记录) | 已执行；19 项 HTTP 回归、真实 dev/run 根入口、登录/图库及已授权的隔离浏览器验证 | [前端 Runtime](../architecture/frontend-runtime.md)、[CLI](../operations/cli-design.md)、[上传示例](../../examples/uploads-app/README.md) |
| [changes/0019：0.2.0 源包与跨平台二进制发布](../changes/0019-crates-and-binary-release.md#执行验证记录) | 已执行；独立包编译、六目标打包、归档/校验和及 cargo-binstall 安装版本核验，未运行测试套件 | [发布操作](../operations/releasing.md)、[版本日志](../changes/CHANGELOG.md) |

## 初始审查与准备计划

| 文档 | 记录状态与用途 | 后续阅读 |
|---|---|---|
| [阶段 00 首轮验证报告](verification-report.md) | 参考；原始骨架审查，不代表当前功能 | [阶段 00 执行记录](0001-stage00-probes.md)、[当前阶段状态](../development/development-roadmap.md) |
| [P01 依赖与探针计划](p01-dependency-plan.md) | 参考；保留当时待批准的准备事实，不作为已执行证明 | [阶段 00 执行记录](0001-stage00-probes.md) |

## 阶段验证记录

下表按形成顺序保留本目录原文件编号，编号与阶段号、变更记录编号不必相同。结果属于各记录对应的修订和环境，不累加不同阶段的测试数量；后续总验收不改写早期失败或有限覆盖。

| 编号 | 文档 | 记录状态与范围 | 相关代码或配置 |
|---|---|---|---|
| 0001 | [阶段 00 技术探针](0001-stage00-probes.md) | 已执行；最小可行性 | 两包探针与当时独立前端资产 |
| 0002 | [Axum/React/Chromium 同源验证](0002-axum-react-browser.md) | 已执行；显式跨工具验证 | 当时的 p03_http_sqlite.rs、axum-browser.mjs；现行入口见下文 |
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

语言与产品独立测试位于 [KEngine tests](../../crates/kengine/tests/) 和 [Kenvoy tests](../../crates/kenvoy/tests/)，跨 Crate 集成测试及 Rust 浏览器夹具位于 [kenvoy-tests](../../crates/kenvoy-tests/)，浏览器驱动脚本位于 [frontend/scripts](../../crates/kenvoy/frontend/scripts/)。现行工程入口为 [xtask](../../xtask/)，执行命令及前置依赖见[工程与测试流程](../development/engineering-workflow.md)和[测试策略](../development/testing-strategy.md)，项目验证要求见 [AGENTS.md](../../AGENTS.md)。

## 维护、废弃与替代关系

准备计划不证明技术已可用，“已执行”仅表示所记录的验证已发生。定稿后保留命令、退出码、失败与边界，后续结果或勘误另建记录。0012 汇总阶段 00—10 的基础验收，不替代前阶段原始证据，也不覆盖后来新增能力；首版后专项证据从本页单独进入。

旧 tools 与独立前端探针的执行方式已退役，现行替代入口见 [changes/0013](../changes/0013-rust-test-workspace-and-tools-retirement.md)。早期报告中的路径和命令仍是历史事实，复现当前修订时使用现行工程入口，不回写历史输出。

初始报告归档于 [verification-report.md](verification-report.md)，仅迁移位置与修正链接，保留原始审查范围、命令和结论。
