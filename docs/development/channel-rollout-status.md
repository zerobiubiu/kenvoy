# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页。当前仅保留 npm、Homebrew、Scoop；crates.io 与 GitHub Release 主发布流程继续保留。规则见[渠道操作](../operations/distribution-channels.md)，npm 首发见 [0033](../changes/0033-single-npm-package-download.md)，鉴权修复见 [0034](../changes/0034-cargo-auth-and-044-release.md)，0.4.4 正式发行及恢复证据见 [0035](../changes/0035-cargo-package-checksum-recovery.md)。

## 当前发布路径

| 项 | 当前状态 |
|---|---|
| npm 包形态 | `kenvoy` CLI 安装器与纯声明 `@kenvoy/types` 均为 0.5.0；不恢复平台分包 |
| npm 当前版本 | 两包均公开、latest=0.5.0；CLI 为 GitHub Actions/可信发布并附 provenance，类型包为账号首发；实际匿名安装和摘要通过 |
| 0.5.0 发行状态 | CLI 源包、14 项正式 Release 资产与三个渠道均已完成；主运行 42268186543、渠道运行 42273209868 最终成功，首轮失败与恢复保留记录 |
| 旧 Windows 分包 | 最新查询 `kenvoy-win32-x64` 返回 E404，旧包已不在注册表；不再生成或发布任何平台分包 |
| 其它旧分包 | Windows ARM64 发布已停止，其余平台分包未上传 |
| crates.io | Kenvoy CLI 0.5.0 已公开、未 yank；实际下载 574334 字节与官方 SHA256 `ef93592ad532aedcc67ff1201298c906b2db45251f90d6b9a63f97e1ad55b5c0` 一致，包含 28 份同版类型文件；KEngine 保持 0.2.0 |
| GitHub Release | [v0.5.0](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.5.0) 已公开、非预发布；六 ZIP、类型 tarball 与各自侧车共 14 项，类型 tarball 与 registry CLI crate 逐文件一致 |
| Homebrew | formula 0.5.0 已自动推送；四目标实际文件与正式 URL/SHA256 生成结果逐字一致，提交 [18a5e8f](https://github.com/zerobiubiu/homebrew-tap/commit/18a5e8fa8eb8b157388a1b2962438b626703af98) |
| Scoop | 清单 0.5.0 已自动推送；两架构实际文件与正式资产生成结果逐字一致，提交 [f0252fc](https://github.com/zerobiubiu/scoop-bucket/commit/f0252fc81b073efe24afeb22f576a4dea0587dbc) |
| 已退役渠道 | APT、RPM、Alpine、Arch、WinGet、Chocolatey、AUR；不恢复专用实现或凭据 |
| 工作流 | 两个正式入口与六个独立手动打包入口已同步 main；channels 先类型后同版安装器，主发布增加类型 tarball/摘要；本次由 main push 实际触发 |
| 源码版本 | CLI 0.5.0，`@kenvoy/types` 跟随为 0.5.0，KEngine 保持 0.2.0；项目格式仍 0.4.0，新建最低 CLI 为 0.5.0 |
| 0.4.4 发行基线 | v0.4.4 仍指向 6a2d863，原发行资产与旧标签保持不动；0.5.0 不重传或回填该版本 |

取消平台分包和 Linux 软件源不等于取消平台支持：Windows、Linux、macOS 的 x64/ARM64 官方 ZIP 继续保留。安装包约定与具体测试证据以 0033 为准，不以 npm install 退出码 0 推定安装脚本已执行。

本版正式证据：[主发布 42268186543](https://github.com/zerobiubiu/kenvoy/actions/runs/42268186543)、[渠道发布 42273209868](https://github.com/zerobiubiu/kenvoy/actions/runs/42273209868)、[Release v0.5.0](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.5.0)、[npm 类型包](https://www.npmjs.com/package/@kenvoy/types)。详细摘要与验收见 [0044](../changes/0044-cli-0.5.0-and-types-first-publish.md)；历史 0.4.4 保留于 [0035](../changes/0035-cargo-package-checksum-recovery.md)。

本版验收边界：Windows x64 已匿名隔离安装 npm CLI 并真实运行、创建项目、安装注册表类型并检查；类型包生产/开发 JSX 与 52 处非法用法检查通过。Homebrew/Scoop 验证到真实仓库文件和正式摘要，未执行原生安装；其他平台仅宣称 CI 构建与归档验证，不沿用旧版 Linux 运行证据。

## 0.5.0 正式发行与按缺项恢复

main 自动和手动发布均接续全部保留渠道；`manual-release.yml` 取代旧手动发布文件，六个 `manual-package-*.yml` 取代单一打包入口。共享打包与渠道工作流只作为内部实现，不增加人工操作。现有 `kenvoy` OIDC 仍绑定 `channels.yml`，不改动其权限或凭据；新类型包须独立授权。

版本提交与不可变 v0.5.0 均为 `d5e0471bb73222fe53916f18abb8e5d5555ccf12`。先推新标签和 dev，再快进 main，发行后回到 dev。主发布首轮 macOS x86_64 无 runner、无步骤而取消；注解明确 hosted runner 容量不足，仅恢复该 job 及依赖 Release，源包和其余五目标没有重传或重建。主运行最终成功，准备证据见 [0043](../changes/0043-cli-0.5.0-release-preparation.md)，真实发行见 [0044](../changes/0044-cli-0.5.0-and-types-first-publish.md)。

## 独立开发类型包（0.5.0，已首次发布）

`@kenvoy/types` 的版本只取 CLI Cargo 元数据；本次随 CLI 使用 0.5.0，不增加独立手工版本。已移除 `kenvoy types` 和 `.kenvoy/types` 接线，五个 Bun 示例通过同版本本地 tarball 开发依赖消费真实 SDK/JSX 声明。

0.5.0 已先发布包含完整声明目录的 CLI crate，再从官方 checksum 验证通过的归档提取正式 `kenvoy-types-0.5.0.tgz`，最终账号首发该原始资产；npm CLI 安装器随后由渠道恢复发布。旧版 0.4.4 不具备该目录，仍不能拿 HEAD 补造。

正式类型 tarball 为 11412 字节，SHA256 `1f910a3922aae3f3900090989a4abe96332f5b4bd438c67ed65f820e5baa16d5`。匿名 registry 下载与 GitHub 官方 digest、侧车、registry SHA1/SHA512 均匹配；其 28 文件与已发布 CLI crate 一致。旧本地产物验证仍保留在 [0042](../changes/0042-published-editor-types-package.md)，不与正式资产混淆。

用户完成本机登录和 npm 网站 2FA 后，唯一稳定版本上传返回成功；后台处理期间首次匿名 GET 为 404，没有重发，最终精确版本与 latest 均公开为 0.5.0。渠道首轮因新类型包尚无独立可信授权而停止，仅恢复 npm job；类型已存在则跳过，CLI 安装器实际完成 GitHub OIDC 上传，Homebrew/Scoop 成功项保留。

信任配置首次授权回调过期；新授权后创建返回 409，表明已有匹配绑定。随后 `npm trust list` 实际读回独立配置 `f0143407-a725-417e-8840-6b8a278f9531`，仓库/文件匹配，并保留既有 publish、stage、dist-tag 权限，没有删除或覆盖。手动首发、配置读回和 CI 跳过已有类型版本，都不等于类型包 OIDC 实发验证；若绑定尚未完成过首次 OIDC 上传，需遵守 npm 两日激活期限，超期在下次发版窗口重建。


## npm 安装与脚本审批

npm 12 默认阻止未获准的依赖安装脚本。全局安装应显式仅批准本包：

```text
npm install -g kenvoy --allow-scripts=kenvoy
kenvoy --version
```

局部项目使用 `npm install-scripts approve kenvoy` 后执行 `npm rebuild kenvoy`；全局重建使用 `npm rebuild -g kenvoy --allow-scripts=kenvoy`。不修改用户全局策略，不放行所有依赖，也不让运行入口偷偷绕过脚本审批联网。

## 凭据与 OIDC

- 仓库仍保留 `CARGO_TOKEN`、`PACKAGES_TOKEN`；不再需要 WinGet、GPG/APK 或 AUR 凭据。旧 `kenvoy-packages` 仓库和本地恢复副本不动。
- 0.2.0 手动首发由用户本机终端完成 npm CLI 认证及提交；未在聊天中收取密码或验证码。退出 0/HTTP 202 不等于正式版本已公开，最终已核验精确版本及摘要。
- 用户已为 `kenvoy` 创建 Trusted Publisher：GitHub `zerobiubiu/kenvoy`、工作流文件 `channels.yml`，允许 npm publish、npm stage publish 与 npm dist-tag；不改动用户选择的权限。
- `kenvoy@0.5.0` 的实际注册表发布者为 GitHub Actions，trustedPublisher=github，附 SLSA provenance；未新增 NPM_TOKEN，也未修改现有 Secret。provenance 元数据不等于独立密码学验签。
- `@kenvoy/types` 已有独立 GitHub 配置：`zerobiubiu/kenvoy`、`channels.yml`、无 environment，权限为 `createPackage/createStagedPackage/manageDistTags`；均读回确认并原样保留。该包本次由账号首发，未借 CLI 的 OIDC 证据宣称其已实发验证。

## 后续自动发布

后续新版本在 dev 完成验证与版本提交，先推版本标签，再快进并推送 main。首次源包仍须标签树与发布树一致，CLI crate 携带同版完整类型目录；主发布补齐 14 资产后接续三个渠道，npm 内先类型后同版安装器。发版窗口先核对新类型可信绑定是否已激活或需重建；不扫描或补发历史源码版本。

已经正式发布的源包不可重传。0.4.4 的发行后修补与本次 0.5.0 新能力发行分开记录，不能用新源码补造旧版类型；旧版恢复仍从实际 registry 包构建，并按已发布资产检查缺项。0.4.3 保留为未发行标签，不追补发布。
