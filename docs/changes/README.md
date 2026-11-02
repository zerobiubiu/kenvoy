# 变更记录索引

状态：参考（现状文档）。保存每次实施的背景、方案、文件范围、接口变化与验证事实。返回[文档总索引](../README.md)。

## 从哪里开始

- 了解 0.2.0 发布结果：先读[0019：源包与跨平台二进制发布](0019-crates-and-binary-release.md)和[版本日志](CHANGELOG.md)；最近功能修复见[0018](0018-runtime-root-entry.md)，其他实施按下表追溯。
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

## 关联代码与配置

变更集中于 [KEngine](../../crates/kengine/)、[Kenvoy](../../crates/kenvoy/)、[测试成员](../../crates/kenvoy-tests/)、[xtask](../../xtask/) 与 [examples](../../examples/)。原 tools 已退役，旧路径仅作为历史正文中的实施事实保留。文件清单和接口差异保留在各次记录中；当前路线见[实施路线](../development/development-roadmap.md)，产品验收历史见[最终验收](../verification/0012-final-acceptance.md)。

## 维护、废弃与替代关系

“已执行”只表示该次变更已实施，不保证其中方案、进度或告警仍是当前状态。定稿正文不改写、不删除、不重排编号；后续变化或勘误另建记录，并维护本索引的关联关系。

0013 将 tools 中的工程检查、HTTP 场景及浏览器进程夹具迁入 Rust，独立前端探针由正式应用浏览器流程接替；0001 等记录中的旧命令保留为历史事实。0006 中当时尚未消费的发布接口已由 0010 接入；0007 的后台与后续修补验收继续记录在 0008 及最终验证中。这些后续进展保留原记录，不回写历史正文。

0016 将既有产品及前端内部源码迁入 DDD 目录，并以 AST/文件规模门禁约束维护；替代旧平铺实现路径，不改变四成员、KEngine/SDK 公共入口或 0014 的上传与图片行为。0015 的正式示例保持可运行，旧记录的实施路径继续保留历史含义。
