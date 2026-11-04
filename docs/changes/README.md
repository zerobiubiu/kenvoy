# 变更记录索引

状态：参考（现状文档）。保存每次实施的背景、方案、文件范围、接口变化与验证事实。返回[文档总索引](../README.md)。

## 从哪里开始

- 了解当前分支、标签与发布规则：先读[0021：dev/main 双分支与当前版本发布](0021-dev-main-release-rules.md)和[发布操作](../operations/releasing.md)；0.2.0 已发布事实见[0019](0019-crates-and-binary-release.md)及[版本日志](CHANGELOG.md)。
- 核对执行证据：进入[验证记录索引](../verification/README.md)，其中同时索引独立验收报告和变更记录内的专项验证；当前有效行为从[整体架构](../architecture/architecture.md)及对应主题文档查询。
- 追溯实现过程：按下表编号顺序阅读，不把早期阶段的进度或告警当作当前状态。

## 记录清单

下表保留每次实施的原编号与范围。“已执行”描述当次事实，不能代替当前接口契约；关联的后续记录见本页末尾。0012—0019 的专项验证随各自变更正文保存，不另复制为验证报告。[CHANGELOG](CHANGELOG.md) 是持续维护的版本概要，不是另一份不可变执行记录。

| 编号 | 记录 | 记录状态与范围 | 关联实现 |
|---|---|---|---|
| 0001 | [阶段 00 技术探针](0001-stage00-probes.md) | 已执行；最小技术验证 | 两包 tests、frontend probe、Cargo/ignore/AGENTS |
| 0002 | [KEngine 正式解析基础](0002-kengine-foundation.md) | 已执行；阶段 01 | 公共 API/IR/模块图、消费测试与退出保护 |
| 0003 | [KEngine 通用语义与增量](0003-kengine-semantics.md) | 已执行；阶段 02 | 表达式、作用域、绑定、增量会话 |
| 0004 | [Kenvoy DSL 与基础命令](0004-dsl-cli.md) | 已执行；阶段 03 | 业务 IR、验证、配置与 CLI |
| 0005 | [SQLite Runtime 与迁移](0005-sqlite-runtime.md) | 已执行；阶段 04 | 文件数据库、CRUD、迁移、备份与 CLI |
| 0006 | [HTTP、认证与受控 API](0006-http-auth.md) | 已执行；阶段 05 | HTTP、Auth、Session、权限与最小 dev |
| 0007 | [通用 React 表单与内嵌资产](0007-react-forms.md) | 已执行；阶段 06 | frontend、内嵌资产、共享校验、同源 API |
| 0008 | [管理后台与真实浏览器闭环](0008-admin-browser.md) | 已执行；阶段 07 | Admin、共享表单/表格、真实 Chromium |
| 0009 | [TypeScript 扩展与受限执行](0009-typescript-extensions.md) | 已执行；阶段 08 | 自定义组件、源码切片、TS/QuickJS、Action |
| 0010 | [CLI 完整生命周期](0010-cli-lifecycle.md) | 已执行；阶段 09 | 开发监督、产物、HMR 与生产宿主 |
| 0011 | [最终验收与边界修复](0011-final-acceptance.md) | 已执行；阶段 10 | 配置恢复、诊断、配额、统一回归与文档 |
| 0012 | [原生组件与联动选项](0012-native-components-and-options.md) | 已执行；首版后扩展 | select、系统重载、统一数据绑定、原生 CSS 与用户 React 库 |
| 0013 | [Rust 测试成员与工程入口](0013-rust-test-workspace-and-tools-retirement.md) | 已执行；工程重组 | kenvoy-tests、xtask、浏览器协议、tools 退役 |
| 0014 | [多文件上传与图片组件](0014-file-uploads-and-images.md) | 已执行；E04 及本场景 V01 | SQLite 附件事务、上传/读取授权、file/image/SDK、真实模式切换与桌面/窄屏 |
| 0015 | [上传与图片可运行示例](0015-uploads-example.md) | 已执行；正式 examples 项目 | 原生上传、URL image、管理员 SDK 图库、样例文件及副本运行验证 |
| 0016 | [DDD 分层与模块拆分](0016-ddd-structure-and-module-split.md) | 已执行；内部结构重组，不变更产品协议 | 领域规则、应用端口/用例、SQLite/JS/文件适配器、HTTP/装配、前端职责拆分与 AST/规模门禁 |
| 0017 | [Windows 密码提示乱码修复](0017-windows-password-prompt-encoding.md) | 已执行；交互终端输出兼容修复 | 中文提示使用 Unicode 终端输出，保留隐藏读取、二次确认及密码策略 |
| 0018 | [Runtime 根入口修复](0018-runtime-root-entry.md) | 已执行；通用页面入口修复 | 未声明根页时加载既有 Runtime，保持显式根页权限及未知请求 404；真实 dev/run 和浏览器验证 |
| 0019 | [0.2.0 源包与跨平台二进制发布](0019-crates-and-binary-release.md) | 已执行；两源包、六目标二进制与安装核验 | 独立包构建、真实锁定元数据、Zig/MSVC 分发、crates.io 与 cargo-binstall |
| 0020 | [许可证正文单一来源](0020-license-single-source.md) | 已执行；打包元数据与仓库布局 | license-file 继承、删除两份 crate LICENSE 副本、重复文件排查 |
| 0021 | [dev/main 双分支与当前版本发布](0021-dev-main-release-rules.md) | 已执行；本地实施与定向验证，未推送 | 自动本地版本标签、main 当前版本自动/兜底发布、共享单目标打包与资产契约 |
| 0022 | [根提交声明的版本标签补齐](0022-root-version-tag.md) | 已执行；本地 v0.1.0 与来源核对 | 初始化标签与根的真实产品版本标签共存，修复漏标与错误拒绝 |
| 0023 | [版本标签规则定稿与初始化标签退役](0023-version-tag-rules.md) | 已执行；真实版本标签与持续补齐时机 | 删除人工初始化标签特例，提交/切换/合并/推送前共用维护入口 |
| 0024 | [多渠道软件包发布与自动流水线](0024-package-channels.md) | 已执行；本地产物生成与验证，未上传 | npm/Homebrew/Scoop/WinGet/Chocolatey/AUR 与四类 Linux 仓库的独立 job 与凭据契约 |
| 0025 | [dev/run 优雅退出与安全收尾](0025-graceful-shutdown.md) | 已执行；本地实现与 Windows Ctrl+C 复核，未合并 main | 终止信号、启动取消、HTTP/运行时关闭期限、Vite 回收与开发目录清理 |
| 0026 | [应用兼容、正式迁移与运行生命周期](0026-application-migration-lifecycle.md) | 已执行；本地实现与隔离数据库验收，未发布 | SemVer 双重门禁、产物 v2、正式迁移链、事务/备份、旧库基线、生命周期锁与开发快照 |
| 0027 | [项目原生国际化与 CLI 0.3.0](0027-native-project-i18n.md) | 已执行；原生配置/词典/SDK 与隔离应用验证，未发布 | i18n 工具、双语 create、内置文案/机器错误、产物 v3、授权词典投影与语言状态隔离 |
| 0028 | [强制原生语言、集中译文与 Schema](0028-mandatory-languages-and-schema.md) | 已执行；按用户新契约切换，未发布 | CLI 0.4.0、languages/alias/display、draft 2020-12 校验、启动前门禁、设备语言、产物 v4 及五例迁移 |
| 0029 | [多渠道发布配置续接与安全门禁](0029-channel-rollout-continuation.md) | 已执行；六项渠道 Secret 与专用密钥、本地安全修补和验证，未上传 | npm/WinGet 显式发布、同版本 Linux 包保留、包与索引签名、源码树与账号阻断记录 |
| 0030 | [退役 Chocolatey 与 AUR 渠道](0030-remove-chocolatey-and-aur.md) | 已执行；删除工作流 job、生成器、测试与现行文档，未发布 | 现行渠道收敛为 npm/Homebrew/Scoop/WinGet 与 APT/RPM/Alpine/Arch 仓库 |
| 0031 | [发布版本对齐：CLI 0.4.1 与 AUR 凭据清理](0031-release-version-alignment.md) | 已执行；版本、锁文件、断言与文档对齐，未推送 | CLI 0.4.1、`v0.4.1` 本地标签、删除 AUR Secret、0.4.0 并入本版 |
| 0032 | [退役 Linux 软件源与 WinGet，准备 CLI 0.4.2](0032-retire-linux-and-winget-channels.md) | 已执行；移除五类发布源及四项专用 Secret，保留渠道通过本地验证，未发布 | npm/Homebrew/Scoop、六目标 ZIP、0.4.2 版本与本地标签安排 |

## 关联代码与配置

变更集中于 [KEngine](../../crates/kengine/)、[Kenvoy](../../crates/kenvoy/)、[测试成员](../../crates/kenvoy-tests/)、[xtask](../../xtask/) 与 [examples](../../examples/)。原 tools 已退役，旧路径仅作为历史正文中的实施事实保留。文件清单和接口差异保留在各次记录中；当前路线见[实施路线](../development/development-roadmap.md)，产品验收历史见[最终验收](../verification/0012-final-acceptance.md)。

## 维护、废弃与替代关系

“已执行”只表示该次变更已实施，不保证其中方案、进度或告警仍是当前状态。定稿正文不改写、不删除、不重排编号；后续变化或勘误另建记录，并维护本索引的关联关系。

0013 将 tools 中的工程检查、HTTP 场景及浏览器进程夹具迁入 Rust，独立前端探针由正式应用浏览器流程接替；0001 等记录中的旧命令保留为历史事实。0006 中当时尚未消费的发布接口已由 0010 接入；0007 的后台与后续修补验收继续记录在 0008 及最终验证中。这些后续进展保留原记录，不回写历史正文。

0016 将既有产品及前端内部源码迁入 DDD 目录，并以 AST/文件规模门禁约束维护；替代旧平铺实现路径，不改变四成员、KEngine/SDK 公共入口或 0014 的上传与图片行为。0015 的正式示例保持可运行，旧记录的实施路径继续保留历史含义。

0021 以 dev/main 双分支和只发布当前 main 版本的规则替代历史分支/历史补发方案，删除 distribution.yml，并纠正未充分证明的 Actions 平台限制结论；0019—0020 的原始执行事实保留不重写。

0022 修正 0021 对根提交版本的排除：初始化 v0.0.0 不阻止真实产品版本标签，v0.1.0 按发布包来源指向同一根提交。

0023 按用户新规则退役并删除人工初始化 v0.0.0，只维护已提交的真实版本标签；v0.1.0 的发布来源与对应根提交保持不变。

0026 以随应用交付的正式迁移链替代 0005/0010 的生产启动动态 Diff 方案；保留旧结构审计，经显式基线接入继续使用，不回写早期实现与验证事实。

0027 原生承接 kenvoy 项目国际化并将 CLI 递增至 0.3.0；产物 v3 替代 0026 的 v2，Application IR 与 Schema 哈希保持原协议，不改变正式迁移和基线隔离边界。

0028 根据用户新要求替代 0027 的可选 i18n、分语言文件和提取命令：原生语言始终启用，集中译文强制引用生成 Schema，无源码也须保留根文件。0.3.0 的记录不重写；新契约与证据以 0028 为准。

0029 承接 0024 的暂停点，补齐已授权凭据和专用密钥，修复发布错误处理、签名与历史保留；未完成账号关联、版本安排或实际渠道发布，后续状态以持续维护的渠道进度为准。

0030 按用户要求退役 Chocolatey 与 AUR：删除两渠道的 job、生成器、测试与现行文档，现行范围收敛为 npm、Homebrew、Scoop、WinGet 与 APT/RPM/Alpine/Arch 仓库。0024、0029 中关于两渠道的实施与待办保留为历史事实，不再代表当前计划。

0031 解决源码发布门禁与既有标签树不一致的问题：按用户选择递增 PATCH 至 0.4.1 并创建新标签，不移动 `v0.4.0`；0.4.0 的产品内容随 0.4.1 交付，删除已无消费者的 AUR Secret。

0032 按用户进一步要求取消 APT、RPM、Alpine、Arch 与 WinGet，并经确认准备 CLI 0.4.2；0024、0029、0030 的旧发布范围不再作为操作依据。历史实施和验证事实不变，仅将已删除源码的失效链接定位到其原始提交；当前配置统一见渠道操作与进度页。
