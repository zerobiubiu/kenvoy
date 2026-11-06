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
| 0033 | [npm 单包按平台下载与 CLI 0.4.3](0033-single-npm-package-download.md) | 已执行；单包安装器、0.2.0 正式 npm 首发及 Windows/Linux 注册表安装、0.4.3 本地验证 | 移除平台分包、下载/摘要/原子安装、Ctrl+C、npm 12 审批、单包 OIDC 与旧分包撤销状态 |
| 0034 | [crates.io 鉴权修复与 CLI 0.4.4 发布恢复](0034-cargo-auth-and-044-release.md) | 已执行；定位 0.4.3 上传前失败，修复 Token 接线并验证真实 Cargo 请求，准备 0.4.4 | 默认 registry 凭据、失败前/通过后回归、不可变标签与正式发布恢复 |
| 0035 | [发布后校验与草稿 Release 恢复](0035-cargo-package-checksum-recovery.md) | 已执行；0.4.4 源包、六目标 Release、npm OIDC 与 Homebrew/Scoop 发行及安装证据 | 上传前摘要、真实 Cargo 发布、草稿分页检索、既有产物恢复与 14 项回归 |
| 0036 | [完整发布与六个手动打包入口](0036-workflow-entrypoints.md) | 已执行；本地入口与文档修正，未推送、未触发远端 | 自动/手动全渠道接续、八个业务入口、共享单目标打包与分支边界 |
| 0037 | [翻译引用预检与 React 函数模板](0037-i18n-reference-check-and-react-templates.md) | 已执行；本地源码、CLI/HTTP、浏览器与回归验收，未发布 | 缺词启动阻断、useLocale、静态函数入口、模板闭包及并发夹具隔离 |
| 0038 | [编辑器 SDK 类型与 JSX Runtime](0038-editor-types-and-jsx-runtime.md) | 已执行；原生声明交付、CLI 配置保护、真实 TypeScript 正反向与浏览器验收，未发布 | types/create、SDK 声明闭包、DSL/HTML/SVG 类型隔离、共享 React 与工具链缓存升级 |
| 0039 | [五个示例的编辑器类型生成](0039-example-editor-types.md) | 已执行；五例声明与配置生成、摘要保护核对，未安装依赖 | 五份 tsconfig、三份新开发清单、两份原清单保留与本地派生声明 |
| 0040 | [五个示例的开发类型安装与检查](0040-example-typecheck-dependencies.md) | 已执行；显式授权安装、五例 tsc 正反向与静态资源模型复核 | 开发依赖/锁文件、CSS 副作用声明、静态 JSX 插值及用户暂存保护 |
| 0041 | [examples 切换 Bun 包管理](0041-examples-bun-package-manager.md) | 已执行；按用户要求仅切换，未验证 | 五份包管理器声明、Bun 锁替换 npm 锁、示例命令与框架边界说明 |
| 0042 | [独立编辑器类型包与同版发布](0042-published-editor-types-package.md) | 已执行；真实 tarball、Bun 五例、新建项目与本地发布边界验证；未首次发布 | `@kenvoy/types` 单一版本源、包管理器接线、移除 types/cache、同版 registry 源包和类型优先渠道 |
| 0043 | [CLI 0.5.0 与类型包同步发行准备](0043-cli-0.5.0-release-preparation.md) | 已执行；本地版本、完整标准 Rust/前端套件、五例检查与首发前置；尚未上传 | CLI/types 0.5.0、KEngine 0.2.0、i18n 最低 CLI、同版正式来源与首次会话/TP 顺序 |

0041 记录切换 Bun 时提交锁文件的历史事实；后按用户要求，examples 下的锁文件不再由 Git 跟踪，本地文件保留。当前安装方式见[示例总览](../../examples/README.md)。

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

0033 按用户要求将 npm 改为一个 `kenvoy` 包，安装时下载当前平台的正式归档；替代 0024、0029 中的平台分包方案。0.2.0 手动首发、旧分包撤销与尚未发布的 0.4.3 源码分别记录，不混同为一次已完成的远端发布。

0034 记录 0.4.3 实际发行在 crates.io 上传前失败的原因，并按用户选择修复后改发 0.4.4。0.4.3 标签与 0033 原始事实保留，不把本地 npm 首发或编译通过等同于后续渠道发行成功。

0035 记录 0.4.4 已上传源包后的校验路径问题及草稿检索问题，保留发布源与标签，复用已构建产物完成发行。后续工程修补不重传源码、不再次递增产品版本；0034 的当时状态与失败事实保留。

0036 按用户新要求替代 0021 的旧手动入口及 0024 的渠道独立手动补发：正式发布只有两个完整入口，手动打包拆成六份。历史记录保留原事实；当前入口、操作命令与本地/远端状态以现状文档为准。

0037 收紧 0028 中显式业务 `t(...)` 调用的缺词行为：源码预检必须通过才能构建或启动，不能依靠 fallback 掩盖漏配；自动 DSL 默认文字和词典矩阵规则保持。新增 `useLocale()` 与静态 React 函数模板入口，不改变 KEngine、数据迁移协议或既有 0.4.4 发行事实。

0038 为 0037 的 React 模板与原生 SDK 补齐编辑器交付：真实声明随 CLI 分发，create/types 建立 TypeScript 接线，保持运行时 React 共享与既有用户配置保护；不以空壳类型或关闭检查代替正确解析，不改变已有发行与标签。

0042 替代 0038/0039 的 CLI 内嵌声明和 `kenvoy types` 缓存交付；SDK/JSX 的严格语义不变，改由同版 npm 开发类型包消费。0040 的依赖与 CSS/JSX 修正继续有效，0041 的 Bun 管理和后续不跟踪示例锁文件约定保持；旧正文不改写。

