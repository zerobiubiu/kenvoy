# 产品版本日志

状态：参考；治理：持续维护的版本概要。详细执行事实保留在编号变更记录中，不以本页替代验证证据。

## 0.4.4

发布状态：待正式发行验证；经用户确认替代上传前失败的 0.4.3，KEngine 保持 0.2.0，项目格式仍为 0.4.0。

- 修复 crates.io 上传凭据接线：将仓库 `CARGO_TOKEN` 仅交付给实际上传子进程的 `CARGO_REGISTRY_TOKEN`，不向编译或前端暴露凭据。
- 增加真实 Cargo、本机 registry 和实际鉴权请求回归；原实现被拒绝，修复后收到正确鉴权，不上传测试包。
- 继续交付此前有效的迁移、强制 i18n、dev/run 生命周期及 npm 单包能力，不恢复已退役渠道；保留所有旧标签，不单独补发旧版本。恢复记录见 [0034](0034-cargo-auth-and-044-release.md)。

## 0.4.3

发布状态：未发行；源包编译通过，但正式上传前因凭据变量错误停止。继续有效的实现由 0.4.4 交付，保留已推送的 v0.4.3，不重指或补发。下述内容保留为该版本的实现事实。

- npm 改为只发布 `kenvoy` 单包，安装时自动选择当前系统/架构并下载对应正式 Release ZIP；删除平台分包及其依赖和发布循环。
- 下载器固化并验证 ZIP、二进制的长度与 SHA-256，支持 HTTP(S) 代理及 NO_PROXY，按二进制摘要隔离缓存后原子写入；不运行失败安装遗留的旧版本。
- npm 12 安装需要显式授权本包脚本，例如 `npm install -g kenvoy --allow-scripts=kenvoy`；不全局放行脚本，也不通过首次运行偷偷绕过脚本审批。
- 包装器保留命令与退出行为，避免 Windows Ctrl+C 被再次转发后强杀子进程。源码、验证与独立的 0.2.0 npm 首发过程见 [0033](0033-single-npm-package-download.md)。

## 0.4.2

发布状态：未发布；本节保留当时 0.4.2 的准备事实。继续有效的实现由后续版本交付，不单独补发；原 npm 分包设计已由 0.4.3 的单包下载方案替代。

- 按用户要求退役 APT、RPM、Alpine、Arch、WinGet：删除工作流、生成器、发布器、签名/索引构建和专用测试，渠道只保留 npm、Homebrew、Scoop。
- 保留 crates.io、GitHub Release 六目标 ZIP、npm 七包以及 Homebrew/Scoop 清单；不取消 Linux/macOS/Windows 平台二进制。
- 经授权删除 WINGET_TOKEN、GPG_PRIVATE_KEY、GPG_PASSPHRASE、APK_SIGNING_KEY；保留 PACKAGES_TOKEN、CARGO_TOKEN，远端旧包仓库和本地密钥恢复副本不动。
- 经用户确认递增 PATCH，并在最终版本提交维护本地附注标签 v0.4.2；旧标签不移动。实施与验证见 [0032](0032-retire-linux-and-winget-channels.md)。

## 0.4.1

发布状态：未单独发行；本节保留当时 0.4.1 的准备事实。继续有效的内容拟随 0.4.4 交付，旧标签保持不动；已取消渠道以现行渠道操作说明为准。

- 0.4.0 从未发布：其内容随本版一并交付，本版在其基础上完成渠道工程收尾。
- Chocolatey 与 AUR 渠道退役，工作流、生成器与测试删除，只保留历史记录；现行渠道为 npm、Homebrew、Scoop、WinGet 与 APT/RPM/Alpine/Arch 仓库。
- 为满足源码发布时“标签树与 main 树一致”的门禁，递增 PATCH 版本并在对应提交创建 `v0.4.1`；`v0.4.0` 是未发布的本地标签，保持不动、不推送。详见 [0031](0031-release-version-alignment.md)。

### 工程与分发

- 继续多渠道发布配置：经授权配置仓库 Secret 与专用签名密钥；npm 凭据仍待账号操作，未上传。
- Chocolatey 与 AUR 渠道退役，工作流、生成器与测试一并删除，只保留历史记录；现行渠道为 npm、Homebrew、Scoop、WinGet 与 APT/RPM/Alpine/Arch 仓库。
- 渠道事件限定 main 和本仓库，固定源码提交；npm/WinGet 采用默认只读、显式发布及错误即停止；npm 包补齐原始 LICENSE。
- Linux 仓库保留历史及同版本包，补齐 RPM/Arch 包签名、真实索引别名和快进推送；签名、打包及真实只读计划的证据见 [0029](0029-channel-rollout-continuation.md)。

## 0.4.0

发布状态：未单独发行；继续有效的产品内容拟随 0.4.4 交付。Kenvoy CLI 原为 0.4.0，KEngine 保持 0.2.0。

- 国际化强制开启，使用 languages 的代码/可选唯一 alias/必填 display 与仅代码 fallback，移除旧可选 i18n 配置和分语言目录协议。
- 根 translations.json 集中保存完整语言矩阵；生成 draft 2020-12 language.schema.json，明确必需的 `$schema` 引用与每语言代码/别名 oneOf，使用真实 Schema 引擎校验，不伪造译文。
- `kenvoy i18n` 更新 Schema、仅缺失时初始化译文；dev/run 先检查并更新 Schema，缺少译文或无效内容时在业务启动前阻断。旧 extract/check 子命令移除。
- 页面按设备默认语言精确匹配，不支持时 fallback；使用 configured display，不再优先历史偏好；页内切换仍保留状态。
- 项目格式仅支持 `>=0.4.0, <0.5.0`，最低 CLI 为 0.4.0；产物 4、HTTP 描述 3、词典 Bundle 2；Application IR 1、Schema 哈希 v1 及已固化迁移不变。无源码部署也必须携带匹配的 translations.json。
- 五个示例及回归同步迁移，详细实施与验证见 [0028](0028-mandatory-languages-and-schema.md)；0027 保留为旧方案实施事实，不再代表现行配置。

## 0.3.0

发布状态：未单独发行；本节保留 0.3.0 的实施事实，旧可选 i18n 协议已被 0.4.0 替代。继续有效的内容拟随 0.4.4 交付，KEngine 保持 0.2.0。

### Kenvoy

- 原生支持项目级 i18n：新建应用自带中英文配置和业务词典；DSL 文案自动绑定，内置页面/后台/错误、语言选择、Intl 格式化及自定义 `useI18n()` 共用框架上下文，无需应用自行接库或编写 Provider。
- 增加只读 `i18n extract/check`，校验词典、提取当前声明默认词条并报告缺失；单条回退不改变业务值、权限、Session 或数据库迁移。
- 项目格式支持范围扩展为 `>=0.2.0, <0.4.0`，显式 i18n 要求格式和最低 CLI 均不低于 0.3.0；旧未启用应用保持单中文，不自动提高配置数字。
- 产物协议升级为 3、客户端描述升级为 2，词典与配置摘要纳入构建身份并支持无源码交付；Application IR 1 与 Schema 哈希 v1 不变。实现与证据见 [0027](0027-native-project-i18n.md)。
- 新增完整 [i18n-app 报名示例](../../examples/i18n-app/README.md)，包含双语词典、SDK 插值/复数/格式化、内置按钮覆盖、原生表单/后台和初始正式迁移；实际验证无源码交付与规范值落库。
- 纳入此前未发布的应用版本、最低 CLI 与项目格式独立 SemVer 契约、正式迁移链、冻结数据规则、分步事务、真实基线接入、数据库生命周期锁和隔离开发快照；详细原始事实见 [0026](0026-application-migration-lifecycle.md)，其中产物 v2 被本版 v3 替代。

发布沿用[既有流程](../operations/releasing.md)，本地版本标签不等于远端发布，不补发历史版本。

## 0.2.1

发布状态：未单独发行；本节保留 0.2.1 的实施事实，继续有效的内容拟随 0.4.4 交付。KEngine 代码未变，保持 0.2.0。

### Kenvoy

- 补齐 `dev`/`run` 的优雅退出：启动构建可被终止信号取消；Unix 处理 SIGINT/SIGTERM，Windows 处理 Ctrl+C、Ctrl+Break、控制台关闭与注销。
- 首次信号后按期限排空 HTTP、停止开发 WebSocket、回收开发协调器/Vite、关闭 JavaScript worker 与 SQLite；第二次信号或 75 秒超期强制退出，避免关闭阶段永久挂起。
- Vite 控制与日志读取均有期限；正常关闭后删除本次实例独占的 `.kenvoy/client-dev` 镜像与缓存。
- 实施与验证见 [0025](0025-graceful-shutdown.md)。

### 工程与分发

- 工程流程切换为 dev/main 双分支：本地版本标签自动维护，main 只发布当前版本（含 PATCH），手动发布仅作 main 兜底，不再补发历史版本。
- 手动与正式分发共用单目标打包实现，修复根 LICENSE 输入及新归档名称对应的 binstall 元数据；既有 0.2.0 源包和 Release 资产不覆盖。
- 当时未递增产品版本；本次 0.2.1 统一包含该批工程改动，见 [0021](0021-dev-main-release-rules.md)。
- 修复根提交产品版本漏标：初始化 v0.0.0 与根声明的 v0.1.0 可共存，已有标签不移动；见 [0022](0022-root-version-tag.md)。
- 版本标签仅来自真实已提交版本，及时补到对应版本提交；退役 v0.0.0 初始化特例，增加切换、合并及推送前维护入口；见 [0023](0023-version-tag-rules.md)。
- 新增 npm / Homebrew / Scoop / WinGet / Chocolatey / AUR 与 APT/RPM/Alpine/Arch 仓库发布工作流，只消费已发布 Release 归档；见 [0024](0024-package-channels.md)。

## 0.2.0

发布状态：已发布，2026-11-02；两个 crates.io 源包及六目标预编译二进制均已上线，cargo-binstall 安装核验通过；记录：[0019](0019-crates-and-binary-release.md)。

### KEngine

- 从 0.1.0 的早期占位接口切换到通用 JS/TS/JSX/TSX 语言引擎，不沿用旧 `hello`/`VERSION` 用法。
- 提供 Engine、AnalysisSession、自有 Generic IR、位置与 UTF-16 保真字符串、表达式结构、模块图、作用域/符号/引用及跨模块绑定。
- 增量分析保留候选与最后有效结果，结构化报告语言和模块诊断，不执行用户代码或解释 Web 业务。

### Kenvoy

- 提供声明式 Application IR、资源/字段/页面/权限、真实 SQLite CRUD/迁移、管理员与 Session、受控同源 API。
- 默认原生 React/CSS 表单与后台，支持选项联动、跨资源选项、组件重载、自定义组件 SDK 和受限 JavaScript Action。
- 支持 SQLite 多文件字段、附件权限/绑定/清理、上传图片及 URL 图片展示。
- 提供 create/check/inspect/admin/migrate/build/dev/run、版本化产物、原生监听、受控 Vite/HMR 与错误恢复。
- 完成 DDD 内部分层及模块规模门禁；修复 Windows 密码中文提示和未声明根页的 Runtime 入口。

### 包与分发

- 两个产品版本均更新为 0.2.0，内部测试/工程成员不发布。
- 增加包级 README、MIT 许可证、元数据和精确打包边界；内置前端随源包发布，构建不依赖完整仓库布局。
- 增加 cargo-binstall 元数据、六目标交叉编译和 ZIP/SHA256 分发流程。
- 本次按用户要求不跑测试套件；实际编译、归档与安装核验范围以 0019 为准。

## 0.1.0

crates.io 已有的早期占位版本。归档代码与说明保留在线上版本，不覆盖重传。
