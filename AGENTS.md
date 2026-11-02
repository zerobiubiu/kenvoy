# AGENTS.md — Kenvoy

本文件适用于本仓库及子目录，补充全局约定；当前明确用户指令优先。面向用户的说明、文档、注释和诊断使用简体中文，标识符与技术术语保留英文。

## 称呼与术语（强制）

以下名称各有唯一含义，文档、注释、诊断与对话一律遵守，细则见[称呼与术语](docs/reference/terminology.md)：

- **kenvoy 项目**：使用者用本产品规定的 DSL（`jsx`/`tsx`）编写、含 `kenvoy.toml` 的应用项目。**不是**本仓库、**不是** Workspace、**不是** `crates/kenvoy`。既有文档中的“用户项目”“应用项目”同义。
- **Kenvoy CLI**：`crates/kenvoy` 产品包及其二进制 `kenvoy`。
- **KEngine**：`crates/kengine` Library 包，不区分其他称呼。
- **本仓库 / 本 Workspace**：当前 Git 仓库与四成员 Workspace；不用“本项目”“Kenvoy 项目”指代它。
- **KenvoySpace / `kspace`、Kenvoy-Runtime / `krt`**：规划中的闭源组件（云平台、生产调度器），**当前未实现**，没有接口、仓库或部署方式。文档、README 与回复提到时必须标注“规划中、未实现”，不得描述为可用能力，也不得据此预建代码目录或接口。

指代不清时按此表改写成明确名称，不新造第三种叫法；代码、配置与命令中的英文标识符保持原样。

## 任务范围与入口

- 开工先核对工作区状态、分支和已有改动，再阅读相关规则、配置与实现；按任务需要检查提交历史、`.gitignore` 和 Workspace，不扩大成无关全仓审查。
- 从[文档中心](docs/README.md)进入；完成范围见[阶段路线](docs/development/development-roadmap.md)，后续任务见[实施与验证待办](docs/development/implementation-backlog.md)。归档待办、可选扩展和历史授权不自动扩大本次任务范围。
- 只读任务不得触发构建、安装、生成产物、迁移或写库；仅运行已确认无写入副作用的命令。需要验证写入路径时先取得相应授权，不用现有示例或用户数据库作试验。
- 复用现有实现，只创建实际需要的模块、测试和文档；不预建空脚手架。历史 XML、SSR、PostgreSQL 路线不属于当前能力，未经审查不得恢复。

## 架构与已确认设计

- Workspace 使用四个成员：`crates/kengine`（产品 Library）、`crates/kenvoy`（唯一用户 Binary）、`crates/kenvoy-tests`（跨 Crate 测试）和 `xtask`（工程编排）；默认成员为两个产品包。测试/工程成员禁止发布、不承载产品业务，各产品 Crate 保留独立 tests。
- 产品依赖只能是 `kenvoy → kengine`，通过 KEngine 公共 API 交互；产品不得依赖测试成员或 xtask，KEngine 不得新增 Binary。不得为模块化擅自增加 Workspace 成员，边界见[Crate 职责](docs/architecture/crate-boundaries.md)。
- KEngine 使用 SWC 处理 JS/TS/JSX/TSX、Generic IR、模块图、作用域、符号、引用、位置及诊断；不实现 Web 业务、数据库、CRUD、HTTP、认证、React 或页面语义，不自行重写完整 Parser/TypeScript 类型检查器，不公开 SWC 内部类型作为长期契约。
- Kenvoy 负责 DSL、Application IR、权限、迁移、CLI、构建和运行生命周期。Generic IR 与 Application IR 独立定义和版本化；页面、API、后台与数据库共用已验证业务模型。请求内不重新解析 TSX、构建前端或初始化整库，不按资源/页面生成重复业务源码作为主要执行方式。
- 普通 UI 不参与 Schema 推导；Schema、Client、Server、Shared 模块分开处理。无法求值的表达式保留可追踪引用，Schema 无法静态确定时明确诊断，不忽略字段；服务端实现和敏感配置不得进入客户端产物。
- Kenvoy 默认使用 Rust 2024、Axum/Tokio/Tower、SQLite/rusqlite、rquickjs、React/TypeScript/原生 CSS/CSR，不内置 MUI/Emotion。四成员和默认原生 UI 是已确认设计，不因旧提示词安排回退；用户可选兼容 React 库，重载与数据组件仍复用原生模型、校验、权限及 API。

## 代码组织与拆分

- Kenvoy 源码按 `domain/application/infrastructure/interfaces/bootstrap` 归位；KEngine 按语言域分层，前端按实际 app/domain/application/infrastructure/presentation 职责组织，具体依赖方向见[DDD 目录规范](docs/architecture/ddd-structure.md)。
- 领域只含模型和纯规则；应用通过实际端口工作，不反引基础设施、接口或装配根。驱动、文件系统及框架类型留在边缘，不用空接口、全局 shared 模块或内部旧路径 shim 伪造分层。
- 自有代码文件硬上限 500 行、目标 400 行内，测试同样受限；达到规模边界先按职责拆分再扩展。数据、依赖及生成产物不计代码上限，禁止靠压行、include 字符串或忽略名单绕过门禁。
- 公共根 API/SDK 门面保留原契约；内部调用直接迁移到规范模块。生产 import 精确到实际项，可见性按真实调用最小开放，不用通配符或一律 pub(crate) 兜底。
- 结构调整同步调用方、测试 helper、前端嵌入/镜像清单与文档链接；通过 `cargo xtask architecture` 的目录、规模和依赖门禁，不为重构更改规则、错误时机、序列化顺序或事务边界。

## 依赖、安全与数据

- 本仓库已授权自主选择并安装本工程实施与验证必需的依赖和工具，无须逐项审批；记录版本、feature、来源、位置、用途及锁文件。授权不覆盖无关安装、费用、对外发布或破坏用户数据，也不覆盖当前只读任务禁止的操作。
- 保留锁文件和来源信息，不在每次启动时重新安装。用户应用的依赖安装仍须显式授权（如 `--install-deps`），不得静默改写其依赖声明；具体契约见[组件与选项](docs/contracts/component-bindings.md)。
- SQLite 位于独立持久化目录，dev/run 使用同一路径；构建、部署或解析错误不得删除、重建业务库。迁移先生成 Diff，保留版本/校验值并以事务保护；删除、改名、改型和约束变化不得静默破坏数据，失败必须如实报告回滚或已提交状态。
- 高影响操作先确认对象、范围与后果：删除、不可恢复覆盖、数据迁移、对外发送、费用或安全设置变更；已授权的安全自动迁移仍遵循既定策略。
- 管理 API 必须认证并在服务端授权；公开表单 create 不开放匿名列表、详情、更新或删除。密码安全哈希，不硬编码默认管理员密码；SQL 标识符仅来自验证 IR，输入值使用绑定参数。
- JavaScript 不获得任意宿主权限；不可信代码在更强隔离实现并验证前拒绝执行。QuickJS 资源限额不等于操作系统沙箱。

## Git 与版本

- 常驻分支只有 `dev` 与 `main`：所有开发在 `dev`，不再创建 `dev.MAJOR.MINOR.PATCH`；用户决定发布时才把已确认版本合并到 `main`。`main` 承载最新发布代码，MAJOR、MINOR、PATCH 均可发布；开发分支不得发布。完整流程见[发布操作](docs/operations/releasing.md)。
- 两分支共同根为 `7d3b5c2860d6e3c64948612a53d0df74ddf47883`，其真实产品版本标签为 `v0.1.0`；不改原始提交内容或历史。人工初始化标签 `v0.0.0` 已退役，不再凭根节点身份生成额外标签。
- 一波完整逻辑修改、必要验证通过后，自动创建本地 Git 提交，包含相应源码、测试与文档；中文信息写清做了什么、为什么。只读、无变更或用户明确要求暂不提交时不创建提交；失败不跳过检查或提交钩子。
- 自动提交授权不包含推送、切分支、合并、变基或改写历史；这些仍须明确要求。保留用户未提交改动，不擅自覆盖、删除、stash 或清理；危险操作先备份并确认。
- 产品版本以各产品 `Cargo.toml` 为准，edition/MSRV 等共用属性来自根 manifest；不把 kenvoy 项目的版本、IR 版本当作产品版本，也不因每次源码或文档修改机械升版。版本变更同步锁文件、依赖约束及 CHANGELOG。
- Kenvoy CLI 每次已提交版本变化，都自动创建附注标签 `vMAJOR.MINOR.PATCH`，缺失标签及时补齐到该版本首次出现的提交，不能统一挂到当前 HEAD；同版本普通提交不重打标签。每次可写任务开始/收尾、提交后、切回 dev、合并到 dev 后及获准推送前均核对补齐，无须额外要求；只读任务不写引用。既有合法发布标签不移动、不覆盖，冲突必须报错；删除或重指需单独授权。标签默认只在本地创建，补 tag 不等于发布旧版本。具体命令与钩子见发布操作。
- 获得推送授权后，本次分支与保留的版本 tag 一并同步到 GitHub，不遗漏先前补齐的标签；推送前再运行一次标签核对。标签逐个显式推送，仍不使用 `git push --tags` / `--follow-tags` 批量推送未核对版本。发布新版本前必须确认标签已在远端，指向已合入 main 的相同版本源码；CI 不自动创建或移动远端 tag。
- 仅 `main` 更新自动触发发布；只检查 main 当前版本，不扫描或补发历史版本。已发布源包跳过，缺失的当前版本资产可由同一流程补齐；手动发布仅是 main 自动发布的兜底，不允许从 dev 或任意历史版本发布。
- 渠道发布（npm、Homebrew、APT、RPM、Alpine、Arch、WinGet、Chocolatey、Scoop）只消费已发布 GitHub Release 的六个归档与官方摘要，不重新编译、不改动 Release；由独立工作流在 main 发布成功后触发，也可按已发布版本手动补发。缺 Secret 或外部账号未就绪必须明确失败并指出名称，不得跳过或伪造成功；各渠道审核（WinGet PR、Chocolatey、AUR 首次建仓）属于外部流程，不能写成已可安装。配置见[多渠道发布](docs/operations/distribution-channels.md)。
- 只暂存本次授权且已验证的文件或变更块，不夹带用户已有或无关改动；不提交密钥、个人配置、依赖目录、业务运行数据或构建产物，新增持久化目录前检查 ignore 保护。

## 实施与验证

- 按审查、设计、实施、验证、验收推进；核心验收失败先修复，不叠加后续功能。技术探针保持最小范围，不借验证扩展开发，也不代替产品闭环。
- 按影响选择最低充分验证，不机械全跑：仅文档/本文件使用 `cargo xtask docs`；功能变更执行定向检查和测试，公共 API、Schema、并发及生命周期提高到跨层验证。命令与前置条件见[工程流程](docs/development/engineering-workflow.md)和[测试策略](docs/development/testing-strategy.md)。
- 优先 `--locked`，缓存足够时使用 `--offline`；不关闭错误检查或放宽告警来通过构建。KEngine 变更另做独立 `cargo check -p kengine`、`cargo test -p kengine`，不得要求数据库、HTTP 或前端环境。
- 工程编排使用 Rust xtask；真实 CLI/HTTP/SQLite 断言和进程生命周期由 Rust 测试负责，Playwright 仅驱动浏览器与 DOM 断言，前端单元测试保留原生工具；不恢复已退役 tools 作为现行入口。
- 测试使用自有临时项目、数据库和进程，清理仅限自有资源。浏览器用例须显式执行；普通测试通过不包含 ignored 用例，Windows 强制终止不等于真实 Ctrl+C。
- 只报告实际命令、输出和退出码，区分本轮验证、历史记录、探针与推断；0 tests、mock 或静态检查不能证明运行闭环。失败或无法验证时说明原因、替代检查和残余风险。

## 文档与交付

- `docs/README.md` 为唯一入口，`docs/` 根目录只留该 README，正文按类型归入子目录，每类有 README；类型不在元数据重复。目录/编号/状态等细则按[文档中心规范](docs/README.md)执行，不恢复旧平铺路径。
- 源码变更同步受影响契约/现状文档并新增实施记录；定稿决策、变更与验证保留事实，后续变化另建记录并维护替代关系，迁移只修必要相对链接。纯文档、注释和本文件维护不递增版本、不递归新增变更记录。
- 阶段交付说明完成范围、文件、关键实现、实际验证、限制与下一步；项目相关回复按全局约定附执行记录，标识和证据不得编造。
