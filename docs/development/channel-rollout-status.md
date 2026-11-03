# 渠道发布进度与待办

状态：参考（现状文档）；治理：持续维护的进度页；记录日期：2026-11-03。范围：npm、Homebrew、Scoop、WinGet、Chocolatey、AUR 与 APT/RPM/Alpine/Arch 仓库的首次真实发布。规则见[多渠道软件包发布](../operations/distribution-channels.md)，实施与本地验证见 [0024](../changes/0024-package-channels.md)。

本页只记录“现在到哪一步、下次做什么”，不重复脚本细节，也不把本地生成结果写成已发布。

## 当前状态

| 项 | 状态 |
|---|---|
| 渠道工作流与生成脚本 | 已完成，位于 `dev`（最近提交见 Git 历史） |
| 三个辅助仓库 | 已创建：`homebrew-tap`、`scoop-bucket`、`kenvoy-packages` |
| `kenvoy-packages` 的 `gh-pages` | 已初始化；Pages 因自定义域 DNS 不在 GitHub，改用 raw.githubusercontent |
| 本地生成器回归 | 72 项通过（1 项符号链接用例在 Windows 跳过） |
| 本地 npm shim | 用真实 0.2.0 二进制运行通过 |
| 本地 nfpm 打包 | deb/rpm/apk/pacman 四种包生成成功，DEB/APK 内容已核对 |
| 仓库 Secret | **仅 `CARGO_TOKEN`；8 个渠道 Secret 均未配置** |
| AUR `kenvoy-bin` | **尚未创建**（首次推送自动创建） |
| main 分支 | 仍是旧发布配置；渠道工作流尚未进入默认分支 |
| 真实渠道发布 | **未运行、未上传** |
| Repology | `/api/v1/project/kenvoy` 返回 `[]`；README 徽章当前显示 “No known packages” |

## 下次继续：按顺序执行

1. **配置 8 个 Secret**（命令见[发布操作](../operations/distribution-channels.md#配置步骤windows--git-bash)）：
   `NPM_TOKEN`、`PACKAGES_TOKEN`、`WINGET_TOKEN`、`CHOCOLATEY_API_KEY`、`AUR_SSH_PRIVATE_KEY`、`GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`。
   用 `gh secret list --repo zerobiubiu/kenvoy` 核对。
2. **AUR 账号**：在 <https://aur.archlinux.org> 的 *My Account → SSH Public Key* 登记 CI 公钥；无需网页建包。
3. **准备一个采用新归档命名的版本**：0.2.0 的 Release 是旧的三元组文件名，渠道工作流不兼容；需把产品版本升到下一个版本（例如 `0.3.0`）并生成六目标 `kenvoy-{version}-{platform}-{arch}.zip`。
4. **发布该版本**：在 dev 完成提交与本地 tag，推送所需 tag，合并并推送 main；观察 `main 最新版本自动发布` 成功。
5. **观察渠道工作流**：`多渠道软件包发布` 由发布成功自动触发；若未触发，在 Actions 手动运行并填入该版本。
6. **逐项验收**（每项单独记录，失败不连坐）：
   - npm：`npm install -g kenvoy`、`kenvoy --version`
   - Homebrew：`brew install zerobiubiu/tap/kenvoy`
   - Scoop：`scoop bucket add zerobiubiu https://github.com/zerobiubiu/scoop-bucket` → `scoop install kenvoy`
   - WinGet：确认 PR 合并后 `winget install Kenvoy.Kenvoy`
   - Chocolatey：审核通过后 `choco install kenvoy`
   - AUR：`yay -S kenvoy-bin`
   - Linux：按 [APT/RPM/Alpine/Arch 安装命令](../operations/distribution-channels.md#apt--rpm--alpine--arch-仓库)逐项验证签名与版本
7. **Repology**：至少一个被 Repology 监控的仓库（AUR/Homebrew/发行版等）收录后，徽章才会显示数据，通常有延迟；确认 `https://repology.org/project/kenvoy/versions` 可访问后，README 徽章无需改动。

## 已知边界

- 仓库 Secret 缺失时对应 job 明确失败并指出名称，不会伪造成功；首次运行前不要合并到 main 后盲目重试，先补齐凭据。
- WinGet PR 合并、Chocolatey 审核、AUR 首次建包属于外部流程，不能由本仓库 CI 决定完成时间。
- Linux 仓库通过 raw.githubusercontent 提供，不使用 Pages 域名；若之后为 `zerobiubiu.top` 配置了 `packages` 子域并指向 GitHub Pages，再统一改 base URL。
- 渠道只消费已发布 Release 资产，不重新编译；修改打包脚本不会改变已发布版本的内容。
