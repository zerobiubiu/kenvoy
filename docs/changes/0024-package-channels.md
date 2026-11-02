# 0024：多渠道软件包发布与自动流水线

状态：已执行；治理：不可变变更记录；日期：2026-11-03。范围：Kenvoy CLI 在 npm、Homebrew、Scoop、WinGet、Chocolatey、AUR 与 APT/RPM/Alpine/Arch 仓库的自动发布配置；现行说明见[多渠道软件包发布](../operations/distribution-channels.md)。本轮只在本地生成与验证产物，未在 GitHub 运行渠道发布，也未向任何包管理器上传。

## 背景与确认

用户要求把项目发布到 npm、Homebrew、APT、RPM、Alpine、Arch、WinGet、Chocolatey、Scoop，并配置 CI/CD 自动发布。核实名称与仓库状态：npm `kenvoy`、`@kenvoy/cli` 未被占用；homebrew-core 无 formula；AUR 无 `kenvoy`；WinGet 无清单；三个辅助仓库均不存在。已确认四项：GitHub Pages 单仓库托管 Linux 仓库；npm 主包 + 6 个平台分包；WinGet/Chocolatey/AUR 全自动（凭据由用户提供）；Homebrew 使用二进制 formula。

## 方案与实施

- 辅助仓库已创建（公共）：`zerobiubiu/homebrew-tap`、`zerobiubiu/scoop-bucket`、`zerobiubiu/kenvoy-packages`。
- 新增渠道工作流 [channels.yml](../../.github/workflows/channels.yml)：由 `main 最新版本自动发布` 成功结束触发，也支持按已发布版本手动补发；只消费已存在的 Release 归档，不重新编译或改动 Release。
- 新增资产与产物生成脚本：`assets.py` 读取 Release 官方 digest；`npm_packages.py` 生成 7 个 npm 包（主包 JS shim + 六平台包）；`manifests.py` 生成 Homebrew/Scoop/WinGet/Chocolatey/AUR 文件；`linux_packages.py` 生成 8 份 nfpm 配置与 Arch PKGBUILD；`linux_repo.py` 生成四类仓库索引。
- 渠道 job 独立：npm（`NPM_TOKEN`，已存在版本跳过）、Homebrew/Scoop（`PACKAGES_TOKEN` 推送辅助仓库）、WinGet（`WINGET_TOKEN` fork + PR）、Chocolatey（Windows runner，`CHOCOLATEY_API_KEY`）、AUR（`AUR_SSH_PRIVATE_KEY`）、APT/RPM/Alpine/Arch（`GPG_PRIVATE_KEY`/`APK_SIGNING_KEY`，Pages `gh-pages` 累积发布）。
- 新增 `assets.py` 与 `channels/` 目录；`AGENTS.md`、文档中心与运维索引同步。未改产品源码或版本号。

## 改动文件清单

- 工作流：`.github/workflows/channels.yml`。
- 脚本：`.github/scripts/channels/{assets,npm_packages,manifests,linux_packages,linux_repo}.py` 与 `tests/`。
- 规则与文档：`AGENTS.md`、`docs/operations/distribution-channels.md`、`docs/operations/README.md`、`docs/README.md`、`docs/changes/CHANGELOG.md`、本记录与索引。

## 执行验证记录

环境：Windows x64、Python 3.12.15（uv 托管）、Node 24、nfpm 2.43.0 Windows 二进制；本地无 Docker，未安装 Linux 索引工具。

| 验证 | 实际结果 |
|---|---|
| 渠道生成器测试（4 个文件） | 17 + 22 + 17 + 16 项通过，1 项跳过（Windows 无符号链接）；覆盖资产契约、URL/摘要、os/cpu、shim、Winget portable、Chocolatey 架构选择、AUR 双架构、nfpm 八配置、仓库索引与路径越界 |
| 修复测试夹具缺陷 | Linux 测试最初未写入二进制，导致 5 项失败；修正夹具后全部通过 |
| 真实 0.2.0 归档冒烟 | 用既有六目标二进制构造 fixture，生成 7 个 npm 目录、9 个渠道文件、8 份 nfpm 配置与 PKGBUILD |
| npm shim 实测 | 将主包与 `kenvoy-win32-x64` 放入临时 `node_modules`，执行 `node bin/kenvoy.js --version` 输出 `kenvoy 0.2.0`，退出码 0 |
| 本机 nfpm 打包 | 从真实二进制生成 `kenvoy_0.2.0_amd64.deb`、`kenvoy-0.2.0-1.x86_64.rpm`、`kenvoy_0.2.0_x86_64.apk`、`kenvoy-0.2.0-1-x86_64.pkg.tar.zst` |
| DEB/APK 内容 | DEB `data.tar.gz` 含 `usr/bin/kenvoy` 与 LICENSE；APK 含 `.PKGINFO`（`pkgname = kenvoy`、`pkgver = 0.2.0`、`arch = x86_64`）及同样文件 |
| actionlint | 新工作流与既有工作流均无诊断 |
| 文档 | `cargo xtask check` 退出 0：四成员、依赖方向与规模门禁通过；94 个 Markdown、952 个本地链接通过 |

未验证：GitHub Actions 上的渠道运行、APKINDEX/pacman db/APT Release 的真实工具生成（本地无 dpkg-dev/createrepo_c/apk/repo-add）、Chocolatey `choco pack/push`、WinGet PR、AUR 推送、npm 实际上传。这些必须在首次真实发布时观察，不能由本地生成结果推定已可用。

## 所需凭据与外部前置

仓库 Secret：`NPM_TOKEN`、`PACKAGES_TOKEN`（对三个辅助仓库可写）、`WINGET_TOKEN`（`public_repo`）、`CHOCOLATEY_API_KEY`、`AUR_SSH_PRIVATE_KEY`、`GPG_PRIVATE_KEY`（base64 私钥）、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`（base64 RSA 私钥）。缺任一凭据时对应 job 明确失败并指出名称。

外部前置：AUR 需先在网页创建一次 `kenvoy-bin` 空仓库；WinGet PR、Chocolatey 审核由上游决定；npm 首次发布需账号拥有 `kenvoy` 及六个平台包名。渠道从采用新归档命名的版本开始，0.2.0 旧命名不回溯。

## 后续验证建议

合并到 main 并完成一次真实发布后，逐项核对：npm `npm install -g kenvoy`、`brew install zerobiubiu/tap/kenvoy`、`scoop install kenvoy`、WinGet PR 合并与 `winget install Kenvoy.Kenvoy`、Chocolatey 审核通过、AUR `yay -S kenvoy-bin`、四类 Linux 仓库的 `apt/dnf/apk/pacman` 安装与签名校验。任何一项失败只记录该项，不把渠道整体写成已可用。
