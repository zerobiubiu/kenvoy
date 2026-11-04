# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页；记录日期：2026-11-04。范围：npm、Homebrew、Scoop、WinGet 与 APT/RPM/Alpine/Arch 仓库。规则见[多渠道软件包发布](../operations/distribution-channels.md)，原始实施见 [0024](../changes/0024-package-channels.md)，安全续接见 [0029](../changes/0029-channel-rollout-continuation.md)，Chocolatey 与 AUR 的退役见 [0030](../changes/0030-remove-chocolatey-and-aur.md)。

本页只记录“现在到哪一步、下次做什么”，不重复脚本细节，也不把本地生成结果写成已发布。

## 当前状态

| 项 | 状态 |
|---|---|
| 渠道工作流与生成脚本 | 已完成，位于 `dev`（最近提交见 Git 历史） |
| 已退役渠道 | Chocolatey、AUR：工作流 job、生成器、测试与现行文档已移除，只保留历史记录 |
| 三个辅助仓库 | 已创建：`homebrew-tap`、`scoop-bucket`、`kenvoy-packages` |
| `kenvoy-packages` 的 `gh-pages` | 已初始化；Pages 因自定义域 DNS 不在 GitHub，改用 raw.githubusercontent |
| 本地渠道回归 | 112 项：111 通过，1 项因 Windows 符号链接权限跳过 |
| 本地 npm 包与 shim | 七个包包含 LICENSE；真实 npm 打包预览及 0.2.0 二进制封装运行通过，未上传 |
| 本地 Linux 交付 | 8 个包生成成功；GPG/RSA 实签、Arch 包签名、APT 索引与验签、原包和签名复用通过 |
| 仓库 Secret | `PACKAGES_TOKEN`、`WINGET_TOKEN`、`GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY` 在用；缺 `NPM_TOKEN`；`AUR_SSH_PRIVATE_KEY` 已删除 |
| main 分支 | 渠道工作流尚未进入默认分支；`main` 是 `dev` 的祖先，合并可快进 |
| 真实渠道发布 | **未运行、未上传** |

## 已配置的凭据与密钥

- `PACKAGES_TOKEN`、`WINGET_TOKEN`：经用户授权复用当前 GitHub 登录凭据；三个辅助仓库均已核实有推送权限。没有创建 WinGet fork/PR，也没有对辅助仓库推送。
- `GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`：已生成专用密钥并写入。只回读 Secret 名称和更新时间，不把它描述为可读取密钥值。
- GPG 完整指纹：`D03C849C5A87865D5251A61715A7EFB37DFFE7B8`。
- 受限恢复副本位于当前用户配置目录 `.config/kenvoy-release/keys-20261104-*`，不在仓库中；仅当前 Windows 用户和 SYSTEM 可访问。GPG 口令使用 CurrentUser DPAPI，迁机前须安全导出，不能只复制口令文件到新电脑。
- `AUR_SSH_PRIVATE_KEY` 已按用户授权从仓库删除；旧私钥仅留在本地受限恢复副本中，不再被任何工作流使用。
- 当前版本为 Kenvoy CLI 0.4.1（未发布），`v0.4.0` 是未发布的本地标签，保留但永不推送。
- 本轮不实际发布；登录页已交回用户，不在聊天中收取密码、验证码或 Token。

## 发布路径（用户执行）

1. 版本与标签已对齐：CLI 升为 0.4.1，`v0.4.1` 打在包含该版本的提交上（见下节）。
2. 合并 `dev` → `main`。当前 `main`（`3e870f4`）是 `dev` 的祖先，可快进合并，合并后树与 `dev` 相同。
3. 先推送 `v0.4.1`，再推送 `main`；不使用 `--tags`，也不推送 `v0.4.0`。
4. `main 最新版本自动发布` 成功后，`多渠道软件包发布` 自动触发；失败时按日志逐项修复，不撤销已完成的 Release。

## 发布门禁：版本标签与源码树

- 远端现有 tag 只有 `v0.1.0`、`v0.2.0`；`v0.3.0` 与 `v0.4.0` 是未发布的本地标签，不推送。
- 现行 [require_tag](../../.github/scripts/release_common.py) 在发布新源码时要求 tag 树与 `main` 树完全一致。`v0.4.0` 指向 `4e960afa`（树 `a689af29…`），与后续 `dev` 树不同，因此不能用它发布。
- 已选定合规方案：递增 PATCH 到 0.4.1，并在包含该版本的最终提交打 `v0.4.1`；不改写 `v0.4.0`，也不放宽校验。详见 [0031](../changes/0031-release-version-alignment.md)。
- 该 tag 之后若再有提交，树会再次不一致；因此合并前不要再追加提交，追加则需重新对齐版本与标签。

## 逐项验收（发布后）

- npm：`npm install -g kenvoy`、`kenvoy --version`
- Homebrew：`brew install zerobiubiu/tap/kenvoy`
- Scoop：`scoop bucket add zerobiubiu https://github.com/zerobiubiu/scoop-bucket` → `scoop install kenvoy`
- WinGet：确认 PR 合并后 `winget install Kenvoy.Kenvoy`
- Linux：按 [APT/RPM/Alpine/Arch 安装命令](../operations/distribution-channels.md#apt--rpm--alpine--arch-仓库)逐项验证签名与版本

## 已知边界

- 除 npm 已正确配置 OIDC 的场景外，缺少必要 Secret 会使对应 job 明确失败，不会伪造成功；首次运行前先补齐凭据与源码发布门禁，不盲目重试。
- WinGet PR 合并属于外部流程，不能由本仓库 CI 决定完成时间。
- Chocolatey 与 AUR 已从实现和现行文档移除，不再有对应 job、Secret 消费或安装承诺。
- Linux 仓库通过 raw.githubusercontent 提供，不使用 Pages 域名；若之后为 `zerobiubiu.top` 配置了 `packages` 子域并指向 GitHub Pages，再统一改 base URL。
- 渠道只消费正式 Release 资产，不重新编译；同版本 Linux 包与签名不覆盖，仓库历史不强推。尚未执行 RPM/Alpine/Arch 完整索引与安装、各远端渠道上传及审核，不能把配置完成当成渠道上线。
