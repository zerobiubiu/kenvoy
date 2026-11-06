# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页。当前仅保留 npm、Homebrew、Scoop；crates.io 与 GitHub Release 主发布流程继续保留。规则见[渠道操作](../operations/distribution-channels.md)，npm 首发见 [0033](../changes/0033-single-npm-package-download.md)，鉴权修复见 [0034](../changes/0034-cargo-auth-and-044-release.md)，0.4.4 正式发行及恢复证据见 [0035](../changes/0035-cargo-package-checksum-recovery.md)。

## 当前发布路径

| 项 | 当前状态 |
|---|---|
| npm 包形态 | 现有 `kenvoy` 仍为单个 CLI 安装器、无平台分包；本次准备同步发行 `kenvoy@0.5.0` 与纯声明 `@kenvoy/types@0.5.0`，类型包尚未首次发布 |
| npm 当前版本 | `kenvoy@0.4.4` 已公开，latest=0.4.4；注册表发布者为 GitHub Actions、trustedPublisher=github，附 provenance；Windows/Linux x64 匿名隔离安装通过 |
| 0.5.0 发行状态 | 正式发行准备中，CLI 与类型包尚未实际上传，也未完成该版本的安装验收；不沿用 0.4.4 的成功记录宣称已完成 |
| 旧 Windows 分包 | 最新查询 `kenvoy-win32-x64` 返回 E404，旧包已不在注册表；不再生成或发布任何平台分包 |
| 其它旧分包 | Windows ARM64 发布已停止，其余平台分包未上传 |
| crates.io | Kenvoy CLI 0.4.4 已发布，官方归档下载与 checksum 一致；KEngine 保持 0.2.0 |
| GitHub Release | v0.4.4 已公开，六个 ZIP 与六份 SHA256 完整；实际核验架构、摘要、许可证及 Windows/Linux x64 执行 |
| Homebrew | formula 0.4.4 已自动推送，macOS/Linux 四目标 URL/SHA256 与正式归档一致；提交 151bdf98b60c2c57f423564af31a7b1bcfb7c734 |
| Scoop | 清单 0.4.4 已自动推送，Windows x64/ARM64 URL/SHA256 与正式归档一致；提交 fc92ef75c9d987cb84866fb618654069226e07bb |
| 已退役渠道 | APT、RPM、Alpine、Arch、WinGet、Chocolatey、AUR；不恢复专用实现或凭据 |
| 工作流 | 两个完整发布入口与六个独立手动打包入口不变；dev 的 channels 改为先类型包后同版安装器，主发布增加类型 tarball/摘要；本次 0.5.0 尚未推送或触发远端任务 |
| 源码版本 | CLI 0.5.0，`@kenvoy/types` 跟随为 0.5.0，KEngine 保持 0.2.0；项目格式仍 0.4.0，新建最低 CLI 为 0.5.0 |
| 0.4.4 发行基线 | v0.4.4 固定指向 6a2d863，已发行产物及旧标签保持不动；本次 0.5.0 准备不重传或回填该版本 |

取消平台分包和 Linux 软件源不等于取消平台支持：Windows、Linux、macOS 的 x64/ARM64 官方 ZIP 继续保留。安装包约定与具体测试证据以 0033 为准，不以 npm install 退出码 0 推定安装脚本已执行。

正式证据：[主发布 41922611322](https://github.com/zerobiubiu/kenvoy/actions/runs/41922611322)、[三渠道自动发布 41929186955](https://github.com/zerobiubiu/kenvoy/actions/runs/41929186955)、[Release v0.4.4](https://github.com/zerobiubiu/kenvoy/releases/tag/v0.4.4)。源包与草稿后处理曾失败；按已提交状态恢复后成功，不把早期失败描述为回滚或首次运行全绿。

验收边界：npm 的 Windows/Linux x64 实际安装及 CLI 执行已完成；Homebrew/Scoop 验证到真实仓库提交与清单一致性，未执行两者的原生安装命令；未额外进行 macOS/ARM64 的本机安装验收。

## 0.5.0 工作流准备（本地，未推送）

main 自动和手动发布均接续全部保留渠道；`manual-release.yml` 取代旧手动发布文件，六个 `manual-package-*.yml` 取代单一打包入口。共享打包与渠道工作流只作为内部实现，不增加人工操作。现有 `kenvoy` OIDC 仍绑定 `channels.yml`，不改动其权限或凭据；新类型包须独立授权。

用户已选择同步正式发行新 CLI 与类型包，本次版本安排为 0.5.0，KEngine 相对 v0.4.4 未变、保持已发布的 0.2.0。本次尚未推送或触发 Actions，以上 0.4.4 发行证据不作为新入口或 0.5.0 的远端验收证明。入口实施及此前本地验证见 [0036](../changes/0036-workflow-entrypoints.md)。

## 独立开发类型包（0.5.0，未首次发布）

`@kenvoy/types` 的版本只取 CLI Cargo 元数据；本次随 CLI 使用 0.5.0，不增加独立手工版本。已移除 `kenvoy types` 和 `.kenvoy/types` 接线，五个 Bun 示例通过同版本本地 tarball 开发依赖消费真实 SDK/JSX 声明。

正式资产必须来自同版本已发布 CLI 源码包的完整声明目录；已发布 0.4.4 源码不具备该目录，不能用新 HEAD 补造。首次正式发行安排为 0.5.0：先发布含完整声明目录的 CLI crate，再从该归档提取 `kenvoy-types-0.5.0.tgz`；npm 先类型后同版安装器。旧标签和既有发行资产不变。

此前本地包消费、真实新项目和发布安全边界的验证见 [0042](../changes/0042-published-editor-types-package.md)。该记录使用当时的 0.4.4 本地产物，不代表 0.5.0 已上传或已安装。

当前 npm 注册表对 `@kenvoy/types` 的实际查询返回 HTTP 404；本机 `npm whoami` 实际返回 HTTP 401，登录尚待用户自行完成。用户已确认拥有 `@kenvoy` scope 发布权限，但新包独立 Trusted Publisher 尚未配置、执行或验证，也未执行类型包 npm 上传；现有 `kenvoy` OIDC 发行证据不能自动扩展到新包。不在聊天中提交登录凭据，不伪造 main 或 CI 上下文进行本机首发。

首发顺序不能颠倒：npm 包必须先存在，才能配置独立 Trusted Publisher。待用户完成登录及所需 2FA 后，首次上传必须使用从同版已发布 CLI crate 提取、校验过的正式类型 tarball；确认包公开可见，再通过官方 `npm trust` 等受支持入口配置新包绑定。配置完成与真实 OIDC 发布验证分别记证，不把已存在版本被跳过或手动上传成功写成 OIDC 已通过。


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
- `kenvoy@0.4.4` 已完成真实 OIDC 发布验证：注册表记录为 GitHub Actions 与 trustedPublisher=github，并附 provenance；该次发布不需要新增 NPM_TOKEN。用户选择的 npm dist-tag 权限保留，当时通过 publish 更新 latest，没有额外修改其他标签；这不是 0.5.0 或新类型包的授权证明。
- 用户已确认拥有 `@kenvoy` scope 发布权限；本机登录仍待完成。`@kenvoy/types` 还需独立绑定 `zerobiubiu/kenvoy` 与 `channels.yml`、environment 留空并允许 `npm publish`；本次尚未配置、执行或验证此 Trusted Publisher，未使用或修改现有 Secret。

## 后续自动发布

0.5.0 在 dev 完成验证和版本提交后，先推对应标签，再快进并推送 main。首次源码发布仍须标签树与发布树一致，CLI 源码包须携带同版完整类型目录；主发布补齐六个 ZIP、类型 tarball 及各自摘要后接续三个渠道，npm 内先类型包后同版安装器。上述为发行顺序，尚非已执行结论；不扫描或补发历史源码版本。

已经正式发布的源包不可重传。0.4.4 的发行后修补与本次 0.5.0 新能力发行分开记录，不能用新源码补造旧版类型；旧版恢复仍从实际 registry 包构建，并按已发布资产检查缺项。0.4.3 保留为未发行标签，不追补发布。
