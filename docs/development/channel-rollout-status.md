# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页；记录日期：2026-11-05。当前渠道仅为 npm、Homebrew、Scoop；crates.io 与 GitHub Release 主发布流程继续保留。规则见[多渠道软件包发布](../operations/distribution-channels.md)，本轮退役、凭据清理及版本安排见 [0032](../changes/0032-retire-linux-and-winget-channels.md)。

本页区分发布路径配置、本地验证和实际上线，不将前两者当成已发布。

## 当前状态

| 项 | 状态 |
|---|---|
| 渠道工作流 | 仅 `prepare`、`npm`、`homebrew`、`scoop` 四个 job |
| 保留交付 | GitHub Release 六目标 ZIP、npm 主包及六个平台包、Homebrew formula、Scoop manifest |
| 有效辅助仓库 | `homebrew-tap`、`scoop-bucket`；两个保留渠道共用 PACKAGES_TOKEN |
| 已取消发布源 | APT、RPM、Alpine、Arch、WinGet；加上此前退役的 Chocolatey、AUR，均无 job、发布器或专用生成流程 |
| 旧包仓库 | `kenvoy-packages` 保留，但已无发布消费者，不作为有效安装源 |
| 仓库 Secret | 仅保留 `CARGO_TOKEN`、`PACKAGES_TOKEN`；`NPM_TOKEN` 仍未配置 |
| 产品版本 | Kenvoy CLI 0.4.2、KEngine 0.2.0；项目格式仍为 0.4.0，create 新项目的最低 CLI 为 0.4.2 |
| 本轮验证 | 真实旧版归档生成、保留产物逐字节对照、npm shim、回归与工作流门禁；具体结果见 0032 |
| 远端发布 | 本轮未推送、未合并 main、未 dispatch 工作流、未上传软件包 |

取消 Linux 包管理器源并不取消 Linux 平台二进制：Linux ZIP、npm Linux 平台包与 Homebrew 的 Linux 分支继续保留。

## 凭据与账号

- `CARGO_TOKEN` 用于 crates.io，`PACKAGES_TOKEN` 用于 Homebrew/Scoop，均保留不改。
- 已按本轮授权删除 `WINGET_TOKEN`、`GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`；此前的 `AUR_SSH_PRIVATE_KEY` 已删除。没有撤销用户的 GitHub 登录凭据。
- 旧密钥的本地受限恢复副本保留；不再要求登记公钥、配置 GPG/APK 签名或创建 WinGet PR。
- npm 仍需有效发布授权。尚未配置 `NPM_TOKEN`；若未完成首次建包和可信发布绑定，不能把工作流中有 OIDC 权限当成已经可以发布。按[首次接入说明](../operations/distribution-channels.md#npm-鉴权与首次接入)办理，不在聊天中传递 Token。

## 发布路径（由用户执行）

1. 本轮经用户确认采用 CLI 0.4.2，最终版本提交通过既有钩子维护本地附注标签 `v0.4.2`；旧标签不移动。
2. 合并 `dev` 到 `main` 前，确认目标树与 `v0.4.2^{tree}` 一致。源码发布仍保留[完整 Git 树门禁](../../.github/scripts/release_common.py)，不能只依据版本字符串相同判断可发布。
3. 按发布规则逐个推送本次所需标签，当前版本为 `v0.4.2`，然后推送 `main`；不使用 `--tags`，不补发未发布的旧版。
4. `main 最新版本自动发布` 成功后，自动触发 `多渠道软件包发布`，仅运行 npm、Homebrew、Scoop。

标签对应的是最终版本提交。之后若又改变树内容，发布前须重新处理版本与标签，不能移动旧标签或放宽门禁绕过。

## 发布后的验收

- crates.io / GitHub Release：当前版本源包与六目标 ZIP/SHA256 完整。
- npm：安装 `kenvoy`，核对当前平台包和 `kenvoy --version`。
- Homebrew：安装 `zerobiubiu/tap/kenvoy`，核对版本和对应平台二进制。
- Scoop：从 `zerobiubiu/scoop-bucket` 安装，核对 x64/ARM64 清单与版本。

只对实际执行的渠道记录结果；未运行不记成发布失败，也不将本地测试写成线上成功。0024、0029、0030、0031 保留当时的实施事实，当前范围以本页与 0032 为准。
