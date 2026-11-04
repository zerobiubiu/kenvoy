# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页；记录日期：2026-11-04。范围：npm、Homebrew、Scoop、WinGet、Chocolatey、AUR 与 APT/RPM/Alpine/Arch 仓库的首次真实发布。规则见[多渠道软件包发布](../operations/distribution-channels.md)，原始实施见 [0024](../changes/0024-package-channels.md)，本次续接与证据见 [0029](../changes/0029-channel-rollout-continuation.md)。

本页只记录“现在到哪一步、下次做什么”，不重复脚本细节，也不把本地生成结果写成已发布。

## 当前状态

| 项 | 状态 |
|---|---|
| 渠道工作流与生成脚本 | 已完成，位于 `dev`（最近提交见 Git 历史） |
| 三个辅助仓库 | 已创建：`homebrew-tap`、`scoop-bucket`、`kenvoy-packages` |
| `kenvoy-packages` 的 `gh-pages` | 已初始化；Pages 因自定义域 DNS 不在 GitHub，改用 raw.githubusercontent |
| 本地渠道回归 | 114 项：113 通过，1 项因 Windows 符号链接权限跳过 |
| 本地 npm 包与 shim | 七个包包含 LICENSE；真实 npm 打包预览及 0.2.0 二进制封装运行通过，未上传 |
| 本地 Linux 交付 | 8 个包生成成功；GPG/RSA 实签、Arch 包签名、APT 索引与验签、原包和签名复用通过 |
| 仓库 Secret | 已配置 6 个渠道 Secret，原 CARGO_TOKEN 不动；仍缺 NPM_TOKEN、CHOCOLATEY_API_KEY |
| AUR `kenvoy-bin` | 专用 SSH Secret 已配置；账号公钥未登记，包尚未创建 |
| main 分支 | 仍是旧发布配置；渠道工作流尚未进入默认分支 |
| 真实渠道发布 | **未运行、未上传** |

## 已配置的凭据与密钥

- `PACKAGES_TOKEN`、`WINGET_TOKEN`：经用户授权复用当前 GitHub 登录凭据；三个辅助仓库均已核实有推送权限。没有创建 WinGet fork/PR，也没有对辅助仓库推送。
- `GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`、`AUR_SSH_PRIVATE_KEY`：已生成专用密钥并写入。只回读 Secret 名称和更新时间，不把它描述为可读取密钥值。
- GPG 完整指纹：`D03C849C5A87865D5251A61715A7EFB37DFFE7B8`；AUR 公钥指纹：`SHA256:f6bwtvLP1vxHBDV8d63pjOgP4UHW4hCv2MFOKAopkXY`。
- 受限恢复副本位于当前用户配置目录 `.config/kenvoy-release/keys-20261104-*`，不在仓库中；仅当前 Windows 用户和 SYSTEM 可访问。GPG 口令使用 CurrentUser DPAPI，迁机前须安全导出，不能只复制口令文件到新电脑。
- npm、Chocolatey、AUR 三个登录页已交由用户操作；尚未收到完成登录的确认，不操作已交回的页面，也不索取聊天中的密码、验证码或 Token。

## 下次继续：按顺序执行

1. **补齐两个 Secret**：登录 npm 与 Chocolatey，分别配置 `NPM_TOKEN`、`CHOCOLATEY_API_KEY`。npm 不再支持旧 Automation token；首发及 OIDC 迁移按[发布操作](../operations/distribution-channels.md#npm-鉴权与首次接入)办理。现有七个 npm 包均未创建，不能把未建立的可信发布当作凭据替代品。用 `gh secret list --repo zerobiubiu/kenvoy` 核对名称。
2. **登记 AUR 公钥**：在 <https://aur.archlinux.org> 的 *My Account → SSH Public Key* 追加受限备份中的 `aur_kenvoy.pub`，保留个人公钥，验证 SSH；无需网页建包。
3. **确定正式版本与发布门禁**：dev 当前 CLI 为 0.4.0，未发布；现有 `v0.4.0` 与 HEAD 的完整 Git 树已经不同，首次源码发布会被 `require_tag` 阻断。正式发布时须显式确认版本安排或另行批准规则调整，不能直接宣称推送 main 即可发布 0.4.0，也不能移动旧 tag。本轮仅改工程配置，不机械升版。
4. **按授权发布**：采用六目标 `kenvoy-{version}-{platform}-{arch}.zip` 命名；0.2.0 的旧三元组命名不适用自动渠道工作流。本地旧归档只用于验证，没有重发。确认版本、标签与门禁后，用户另行批准所需 tag 推送及 main 合并/推送；只处理当时 main 当前版本。
5. **观察工作流**：先确认 `main 最新版本自动发布` 成功，再观察自动触发的 `多渠道软件包发布`。渠道工作流当前尚未进入默认分支；本次没有手动 dispatch。
6. **逐项验收**（每项单独记录，失败不连坐）：
   - npm：`npm install -g kenvoy`、`kenvoy --version`
   - Homebrew：`brew install zerobiubiu/tap/kenvoy`
   - Scoop：`scoop bucket add zerobiubiu https://github.com/zerobiubiu/scoop-bucket` → `scoop install kenvoy`
   - WinGet：确认 PR 合并后 `winget install Kenvoy.Kenvoy`
   - Chocolatey：审核通过后 `choco install kenvoy`
   - AUR：`yay -S kenvoy-bin`
   - Linux：按 [APT/RPM/Alpine/Arch 安装命令](../operations/distribution-channels.md#apt--rpm--alpine--arch-仓库)逐项验证签名与版本

## 已知边界

- 除 npm 已正确配置 OIDC 的场景外，缺少必要 Secret 会使对应 job 明确失败，不会伪造成功；首次运行前先补齐凭据、账号关联及源码发布门禁，不盲目重试。
- WinGet PR 合并、Chocolatey 审核、AUR 首次建包属于外部流程，不能由本仓库 CI 决定完成时间。
- Linux 仓库通过 raw.githubusercontent 提供，不使用 Pages 域名；若之后为 `zerobiubiu.top` 配置了 `packages` 子域并指向 GitHub Pages，再统一改 base URL。
- 渠道只消费正式 Release 资产，不重新编译；同版本 Linux 包与签名不覆盖，仓库历史不强推。尚未执行 RPM/Alpine/Arch 完整索引与安装、各远端渠道上传及审核，不能把配置完成当成渠道上线。
