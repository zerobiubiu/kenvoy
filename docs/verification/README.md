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
| [changes/0021：双分支与当前版本发布](../changes/0021-dev-main-release-rules.md#执行验证记录) | 已执行；真实 Git 标签回归、发布策略回归、六目标既有二进制归档、只读远端空跑、源包 dry-run；本轮未推送或运行远端发布 | [Git 与发布操作](../operations/releasing.md)、[工程流程](../development/engineering-workflow.md) |
| [changes/0024：多渠道软件包发布](../changes/0024-package-channels.md#执行验证记录) | 已执行；渠道产物生成、npm shim 实测、nfpm 四格式打包与内容核对；未上传或运行远端渠道 | [多渠道软件包发布](../operations/distribution-channels.md) |
| [changes/0025：dev/run 优雅退出](../changes/0025-graceful-shutdown.md#执行验证记录) | 已执行；Windows 基础/自定义 dev、生产 run 的真实 Ctrl+C，端口/子进程/临时目录清理与定向测试；Unix SIGTERM 与超时路径未实测 | [Runtime 设计](../architecture/runtime-design.md)、[CLI](../operations/cli-design.md)、[前端 Runtime](../architecture/frontend-runtime.md) |
| [changes/0026：应用兼容与正式迁移](../changes/0026-application-migration-lifecycle.md#执行验证记录) | 已执行；SemVer/产物、真实临时 CLI/SQLite/HTTP、分步恢复、启动失败、进程互斥、无源码交付、旧库与快照边界；范围以正文命令为准 | [迁移操作](../operations/migration-design.md)、[配置契约](../contracts/project-configuration.md)、[测试策略](../development/testing-strategy.md) |
| [changes/0027：项目原生国际化](../changes/0027-native-project-i18n.md#执行验证记录) | 已执行；真实 CLI/HTTP/SQLite、词典/产物边界、前端语言/格式化回归及隔离浏览器；具体命令与限制以正文为准 | [原生国际化](../contracts/i18n.md)、[客户端描述](../contracts/application-ir.md)、[前端 Runtime](../architecture/frontend-runtime.md) |
| [changes/0028：强制语言与集中译文](../changes/0028-mandatory-languages-and-schema.md#执行验证记录) | 已执行；配置/别名、真实 Schema、初始化与启动阻断、文件保护、设备语言及集中词典交付；实际范围见正文 | [语言契约](../contracts/i18n.md)、[配置](../contracts/project-configuration.md)、[CLI](../operations/cli-design.md) |
| [changes/0029：多渠道配置续接](../changes/0029-channel-rollout-continuation.md#执行验证记录) | 已执行；114 项渠道回归、主分支门禁、真实只读计划、npm 封装、签名包与 APT 索引；凭据部分完成，远端发布和部分源验收未执行 | [渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0030：退役 Chocolatey 与 AUR](../changes/0030-remove-chocolatey-and-aur.md#执行验证记录) | 已执行；112 项渠道回归、残留引用检查、工作流语法与工程门禁；未运行远端工作流 | [渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0031：发布版本对齐](../changes/0031-release-version-alignment.md#执行验证记录) | 已执行；Workspace 编译、真实 `--version`、framework_version 定向回归、渠道回归与文档门禁；未推送未发布 | [渠道进度](../development/channel-rollout-status.md)、[Git 与发布操作](../operations/releasing.md) |
| [changes/0032：仅保留 npm/Homebrew/Scoop](../changes/0032-retire-linux-and-winget-channels.md#执行验证记录) | 已执行；保留渠道回归、真实产物逐字节对照、npm shim、0.4.2 本机编译和工作流检查；未运行远端发布 | [渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0033：npm 单包与 CLI 0.4.3](../changes/0033-single-npm-package-download.md#执行验证记录) | 已执行；61 项 Python、114 项 Node 回归，正式 npm 包 Windows/Linux 安装及摘要、原生 Ctrl+C、0.4.3 create/check/build、工程门禁 | [渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0034：Cargo 鉴权与 0.4.4 恢复](../changes/0034-cargo-auth-and-044-release.md#执行验证记录) | 已执行；0.4.3 正式失败取证、真实 Cargo 默认 registry 鉴权的失败前/通过后、0.4.4 create/check/build；远端发行状态单独记录 | [发布操作](../operations/releasing.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0035：0.4.4 正式发行与恢复](../changes/0035-cargo-package-checksum-recovery.md#执行验证记录) | 已执行；14 项回归、正式源包校验、六平台归档与头部、npm OIDC、Windows/Linux 匿名安装、Homebrew/Scoop 清单；具体边界见正文 | [发布操作](../operations/releasing.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0036：工作流入口边界](../changes/0036-workflow-entrypoints.md#执行验证记录) | 已执行；本地语法、事件/分支矩阵、真实来源守卫、六目标选择器及既有发布回归；不代表远端调度验收 | [发布操作](../operations/releasing.md)、[渠道操作](../operations/distribution-channels.md) |
| [changes/0037：翻译预检与 React 函数模板](../changes/0037-i18n-reference-check-and-react-templates.md#执行验证记录) | 已执行；完整 Rust 回归、143 项前端测试、真实缺键启动拒绝与恢复、中英文切换/草稿保持及截图 | [语言契约](../contracts/i18n.md)、[DSL](../contracts/dsl-specification.md)、[示例](../../examples/i18n-app/README.md) |
| [changes/0038：编辑器 SDK 与 JSX 类型](../changes/0038-editor-types-and-jsx-runtime.md#执行验证记录) | 已执行；144 项前端测试、真实隔离 TypeScript 正反向、CLI 配置/声明边界、旧缓存升级与开发浏览器 | [编辑器操作](../operations/cli-design.md#编辑器类型)、[SDK/JSX](../contracts/component-bindings.md#编辑器与-jsx-类型)、[前端 Runtime](../architecture/frontend-runtime.md) |
| [changes/0039：示例类型生成](../changes/0039-example-editor-types.md#执行验证记录) | 已执行；五次真实 types、每例 24 份声明摘要、原文件保护核对；未安装依赖或执行类型检查 | [示例状态](../../examples/README.md)、[编辑器操作](../operations/cli-design.md#编辑器类型) |
| [changes/0040：示例依赖与类型检查](../changes/0040-example-typecheck-dependencies.md#执行验证记录) | 已执行；五例依赖安装及 tsc 通过、错误对照仍拒绝、Schema 模型不变；未启动应用或修改业务库 | [示例状态](../../examples/README.md)、[扩展示例](../../examples/extensions-app/README.md) |
| [changes/0041：examples Bun 切换](../changes/0041-examples-bun-package-manager.md) | 按用户要求未验证；仅记录五次锁转换，不代表安装、类型检查或运行通过 | [示例使用说明](../../examples/README.md) |
| [changes/0042：独立编辑器类型包](../changes/0042-published-editor-types-package.md#执行验证记录) | 已执行；真实 tarball TypeScript 正反向、五例 Bun 安装/类型检查、无 Node 的项目创建及同版包消费、发布来源与类型优先边界；未远端发布 | [编辑器操作](../operations/cli-design.md#编辑器类型)、[示例](../../examples/README.md)、[渠道操作](../operations/distribution-channels.md) |
| [changes/0043：0.5.0 同步发行准备](../changes/0043-cli-0.5.0-release-preparation.md#执行验证记录) | 已执行；0.5.0 本地构建、完整标准 Rust 套件、144 项前端、五例 CLI check 与工程门禁；不代表远端发行或 MSRV 验收 | [发布操作](../operations/releasing.md)、[渠道进度](../development/channel-rollout-status.md) |
| [changes/0044：0.5.0 与类型包正式发行](../changes/0044-cli-0.5.0-and-types-first-publish.md#执行验证记录) | 已执行；真实源包/14 资产/注册表双包、Windows x64 匿名 CLI 运行及新项目类型接线、两种 JSX 和 52 错误位置、辅助渠道文件全量对照 | [发行进度](../development/channel-rollout-status.md)、[编辑器操作](../operations/cli-design.md#编辑器类型)、[渠道操作](../operations/distribution-channels.md) |

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
