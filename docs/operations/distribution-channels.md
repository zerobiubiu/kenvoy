# 多渠道软件包发布

状态：参考；治理：现状文档。适用范围：Kenvoy CLI 二进制分发到 npm、Homebrew、APT、RPM、Alpine、Arch、WinGet、Chocolatey、Scoop。产品源包发布仍见 [Git 与发布操作](releasing.md)；原始实施见 [0024](../changes/0024-package-channels.md)，凭据配置与安全续接见 [0029](../changes/0029-channel-rollout-continuation.md)，当前阻断见[渠道进度](../development/channel-rollout-status.md)。以下安装命令只供渠道正式发布后使用，不表示现在已经上线。

## 总体结构

```mermaid
flowchart TD
  A[main 发布成功 GitHub Release] --> B[渠道工作流 workflow_run]
  B --> C[assets.json 六归档 + 官方 sha256]
  C --> D[npm 7 个包]
  C --> E[Homebrew tap formula]
  C --> F[Scoop bucket]
  C --> G[WinGet 清单 PR]
  C --> H[Chocolatey nupkg]
  C --> I[AUR PKGBUILD]
  C --> J[APT/RPM/Alpine/Arch raw 仓库]
```

渠道工作流为 [.github/workflows/channels.yml](../../.github/workflows/channels.yml)：由 `main 最新版本自动发布` 成功结束触发，也支持 `workflow_dispatch` 指定已发布版本补发。`prepare` 在任何 Secret 消费前检查当前 ref、来源分支和来源仓库，拒绝 dev、tag 与外仓事件；后续 job 固定检出 `prepare` 确认的源码提交，不混用运行期间变化的 main。它只消费已存在的 GitHub Release 资产，不重新编译、不改动 crate 或 Release。每个渠道独立 job，单渠道失败不撤销已经完成的 Release。

前置条件：版本 tag 已推送；对应正式 Release 已发布、非 draft/prerelease，六个 ZIP 均带 GitHub 官方 digest。下载和复用缓存都核对 SHA-256，不覆盖损坏缓存。缺归档或摘要不符时停止，不进入各渠道。0.2.0 及更早的 Release 使用旧的三元组文件名，不在自动发现兼容范围内；渠道发布从采用 `kenvoy-{version}-{platform}-{arch}.zip` 的版本开始。

源码发布仍执行现有完整 Git 树一致性门禁：首次发布源码时，main 的树必须与该版本 tag 一致。标签之后仅修改文档或渠道配置，也不能跳过这项检查；正式发布前须显式确定版本安排，不移动旧 tag，也不因本轮工程配置自动提升 CLI 版本。

## 资产与命名

`assets.json` 由 [assets.py](../../.github/scripts/channels/assets.py) 生成，固定六项：`windows/linux/macos × x86_64/ARM64`，文件名 `kenvoy-{version}-{platform}-{arch}.zip`，`sha256` 取 Release 资产 digest。所有渠道只引用这些官方归档，不各自重算二进制。

## npm

发布 7 个**无 scope** 包，避免依赖 npm 组织：

| 包 | 内容 |
|---|---|
| `kenvoy` | 主包：`bin` 为 JS shim，按 `process.platform/arch` 解析平台包，转发参数、stdio 与退出码 |
| `kenvoy-win32-x64`、`kenvoy-win32-arm64` | Windows 二进制 |
| `kenvoy-linux-x64`、`kenvoy-linux-arm64` | Linux musl 静态二进制 |
| `kenvoy-darwin-x64`、`kenvoy-darwin-arm64` | macOS 通用归档中的对应架构 |

主包通过 `optionalDependencies` 依赖六个平台包；平台包声明 `os`/`cpu`，内含真实 `bin/kenvoy(.exe)`。七个包都附带官方归档中的原始 `LICENSE`。

[publish_npm.py](../../.github/scripts/channels/publish_npm.py) 默认只读预检：先校验全部包，再查询全部版本；仅精确 HTTP 404 表示尚未发布，鉴权、限流、网络及服务端异常均失败。显式 `--publish` 且处于 main 才写入，先发布六个平台包、最后发布主包；已有版本跳过，首次失败即停止，不盲目重试或覆盖版本。实际命令保留 `--access public --provenance --ignore-scripts`。

### npm 鉴权与首次接入

- 长期使用 Trusted Publishing（OIDC）：GitHub-hosted runner、`id-token: write`、npm ≥ 11.5.1、Node ≥ 22.14；绑定仓库 `zerobiubiu/kenvoy` 和工作流文件 `channels.yml`，包的 `repository.url` 须对应本仓库。现有工作流让 npm 自行选择 OIDC，缺少 `NPM_TOKEN` 不会提前阻断有效的可信发布。
- 首次创建包与建立绑定是独立前提。七个包尚不存在时，不能声称仅配置 OIDC 就已可发布；首发需由拥有包名权限的账号通过受允许的交互 2FA 或 Granular token 完成。每个包建立 Trusted Publisher 后再撤掉临时 Token。
- 不再创建旧式 Automation token。npm 自 2025 年 11 月起仅支持 Granular token；其绕过 2FA 的直接发布方式将于 2027 年 1 月退役。不得绕过账号或组织的强制 2FA 要求。
- 自 2026 年 9 月起，新可信发布配置还须明确允许工作流执行 `npm publish`，不能只启用 staging。新绑定两天内未首次发布会失效，应在实际发布窗口配置，而非长期提前占位。
- `npm whoami`/`npm view` 不能证明 OIDC 已获发布授权；以真实受授权发布及安装结果验收。

依据：[Access tokens](https://docs.npmjs.com/about-access-tokens/)、[Trusted publishers](https://docs.npmjs.com/trusted-publishers/)（2026-11-04 核对）。

安装：

```text
npm install -g kenvoy
kenvoy --version
```

## Homebrew

tap 仓库 `zerobiubiu/homebrew-tap`，formula 为二进制形式，按 `on_macos/on_linux` 与 `on_arm/on_intel` 选择 Release ZIP，`bin.install "kenvoy"`。不使用源码构建或 bottle 重建。

```text
brew tap zerobiubiu/tap
brew install kenvoy
```

## Scoop

bucket 仓库 `zerobiubiu/scoop-bucket`，清单 `bucket/kenvoy.json` 提供 64bit/arm64 的 URL、hash、`bin: kenvoy.exe`、`checkver`/`autoupdate`。

```text
scoop bucket add zerobiubiu https://github.com/zerobiubiu/scoop-bucket
scoop install kenvoy
```

## WinGet

生成 `manifests/k/Kenvoy/Kenvoy/<version>` 三份 YAML（version/installer/locale），InstallerType `zip` + `NestedInstallerType: portable`，x64 与 arm64 各一条。工作流用 `WINGET_TOKEN` fork 并提交 PR 到 `microsoft/winget-pkgs`；合并由上游审核，不由本仓库决定。

[publish_winget.py](../../.github/scripts/channels/publish_winget.py) 默认仅查询状态并输出计划；`--publish` 还须通过 main 门禁。只向固定上游及已验证的本人 fork 操作，已存在的版本或打开的 PR 不重复提交，允许从仅提交部分清单的自有分支恢复。API/权限错误不按“资源不存在”继续，不强推分支，也不吞掉 fork 或 PR 创建失败。

## Chocolatey

`kenvoy.nuspec` + `tools/chocolateyInstall.ps1`，安装脚本按 `PROCESSOR_ARCHITECTURE` 选择 x64/ARM64 归档、校验 checksum、解压到 tools 并 `Install-BinFile`。使用 Windows runner 的 `choco pack/push` 与 `CHOCOLATEY_API_KEY`；PowerShell 显式检查 Python、pack、push 的退出码，防止后续命令掩盖失败。社区源有审核延迟，推送成功不等于立即可安装。

## AUR

生成 `kenvoy-bin` 的 `PKGBUILD` 与 `.SRCINFO`，`arch=(x86_64 aarch64)`，直接安装 Release 二进制。使用 `AUR_SSH_PRIVATE_KEY` 推送到 `ssh://aur@aur.archlinux.org/kenvoy-bin.git`。

**外部前置**：AUR 账号需在 *My Account → SSH Public Key* 登记 CI 使用的公钥。AUR 没有网页建包流程：CI 首次 `git clone ssh://aur@aur.archlinux.org/kenvoy-bin.git` 后推送 `HEAD:master` 即自动创建 `kenvoy-bin`；只接受 `master` 分支，且必须同时包含 `PKGBUILD` 与 `.SRCINFO`。

## APT / RPM / Alpine / Arch 仓库

托管在公共仓库 `zerobiubiu/kenvoy-packages` 的既有 `gh-pages` 分支，通过 raw.githubusercontent 提供文件（该账号 Pages 绑定的自定义域 DNS 不在 GitHub，暂不使用 Pages 地址）。四个仓库按版本累加；必须先成功读取既有分支，不能将 clone 失败当成空仓库，也不创建 orphan 提交或强推覆盖历史。

基础地址：`https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages`。raw 不提供目录浏览，但 `apt/dnf/apk/pacman` 只需按固定路径取索引与包文件，因此可直接使用。

| 仓库 | 路径 | 索引与签名 |
|---|---|---|
| APT | `/apt` | `dists/stable` + `pool/main/k/kenvoy`；`Packages.gz`、`Release`，GPG 清签 `InRelease` 与 `Release.gpg` |
| RPM | `/rpm` | nfpm 签署每个 RPM；`createrepo_c` 生成 `repodata`，`repomd.xml` 另做 GPG 分离签名 |
| Alpine | `/alpine` | 包与 `APKINDEX.tar.gz` 均使用 APK RSA 密钥签名；公钥随站点发布 |
| Arch | `/arch` | 每个包先生成 `.sig`，`repo-add` 收录包签名；数据库和文件索引再做 GPG 签名 |

[linux_packages.py](../../.github/scripts/channels/linux_packages.py) 接收 RPM 私钥路径及 16 位长 key ID，口令仅通过 `NFPM_RPM_PASSPHRASE` 传递；nfpm 的下载固定版本并核对 SHA-256。[build_linux_packages.py](../../.github/scripts/channels/build_linux_packages.py) 重用既有同版本包和 Arch 包签名，不重新打包已经对外发布的内容；合并时同名文件内容不一致直接失败。

[linux_repo.py](../../.github/scripts/channels/linux_repo.py) 携带 Arch 包签名生成索引。发布前执行 RPM 签名校验与 GPG 验签；Arch 的 `.db`、`.files` 及签名别名写为真实文件，因为 GitHub raw 不会像本地文件系统那样跟随 Git 符号链接。最终仅对既有分支做普通快进推送。

安装示意（以 `stable`/x86_64 为例，实际按发行版调整）：

```text
# Debian/Ubuntu
BASE=https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages
curl -fsSL "$BASE/apt/key.asc" | sudo gpg --dearmor -o /etc/apt/keyrings/kenvoy.gpg
echo "deb [signed-by=/etc/apt/keyrings/kenvoy.gpg] $BASE/apt stable main" | sudo tee /etc/apt/sources.list.d/kenvoy.list
sudo apt update && sudo apt install kenvoy

# Fedora/RHEL
BASE=https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages
sudo rpm --import "$BASE/rpm/RPM-GPG-KEY-kenvoy"
sudo curl -fsSL "$BASE/rpm/kenvoy.repo" -o /etc/yum.repos.d/kenvoy.repo
sudo dnf install kenvoy

# Alpine
BASE=https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages
wget -O /etc/apk/keys/kenvoy.rsa.pub "$BASE/alpine/kenvoy.rsa.pub"
echo "$BASE/alpine/v3.20/main" >> /etc/apk/repositories
apk update && apk add kenvoy

# Arch：先核对项目公钥指纹，将 arch/key.asc 导入 pacman-key 并本地签名；不得关闭签名校验
# /etc/pacman.conf:
# [kenvoy]
# Server = https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages/arch/$arch
```

`kenvoy.repo`、`key.asc`、`RPM-GPG-KEY-kenvoy`、`kenvoy.rsa.pub` 由仓库发布脚本随索引生成。

## 所需仓库 Secret

| Secret | 用途 | 生成方式 |
|---|---|---|
| `NPM_TOKEN` | 首次接入时的过渡发布凭据；配置有效 OIDC 后可省略 | 获准发布七个包的 Granular token；不能使用已退役 Automation token |
| `PACKAGES_TOKEN` | 推送 tap/bucket/gh-pages 仓库 | 对三个辅助仓库有写权限的 GitHub 凭据 |
| `WINGET_TOKEN` | fork 并 PR 到 microsoft/winget-pkgs | 有相应公开仓库操作权限的 GitHub 凭据 |
| `CHOCOLATEY_API_KEY` | 推送 Chocolatey 包 | community.chocolatey.org API Key |
| `AUR_SSH_PRIVATE_KEY` | 推送 AUR | 专用 SSH 私钥；对应公钥须在 AUR 账号登记 |
| `GPG_PRIVATE_KEY` | RPM 包与 APT/RPM/Arch 索引、Arch 包签名 | 独立签名私钥的加密 ASCII 导出，再 base64 |
| `GPG_PASSPHRASE` | 上述专用密钥的口令 | 非空随机口令；经标准输入/环境传递，不放命令参数 |
| `APK_SIGNING_KEY` | APK 包与索引签名 | 专用 RSA 4096 PKCS#1 PEM 私钥，再 base64 |

除 npm 可选择有效 OIDC 外，各 job 缺少必要 Secret 时明确失败，不伪造成功。所有密钥只在对应 job 注入，不打印、不写入发布制品。复用现有 GitHub 登录凭据须获得授权；它的权限可能大于渠道所需权限，后续改为专用最小权限凭据时应单独验证，不擅自撤销现有登录。

### 配置步骤（Windows / Git Bash）

凭据经 `gh secret set` 的交互输入或标准输入直接写入仓库，不经过聊天、源码、命令参数或输出。先查现有配置，不盲目覆盖；本轮完成项与缺口只在[渠道进度](../development/channel-rollout-status.md)维护。

```bash
# npm：先按上文确认 Granular token 与首发策略
gh secret set NPM_TOKEN --repo zerobiubiu/kenvoy

# Chocolatey：在已登录账号的 API Key 页面取得凭据
gh secret set CHOCOLATEY_API_KEY --repo zerobiubiu/kenvoy

# 只列名称与更新时间，不能据此读取或证明 Secret 的值
gh secret list --repo zerobiubiu/kenvoy
```

签名与 AUR 使用专用密钥，不复用个人主密钥，不因续接任务重新生成。生成时使用独立 `GNUPGHOME`、明确的完整指纹以及仓库外的受限目录；不能从用户默认密钥环选择“第一把私钥”。Git for Windows 的 MSYS GPG 通过原生进程调用时须使用 `/c/...` 路径；不为绕过 keyboxd 问题修改用户默认密钥环。

私钥及口令必须有受控恢复副本；临时导出文件用完删除，不能把唯一的恢复副本当临时文件删除。本轮备份目录仅当前 Windows 用户和 SYSTEM 可访问，GPG 口令使用 Windows CurrentUser DPAPI 保护；该口令文件不能直接在另一台电脑或另一账号解密，迁机前须由当前账号安全导出。

AUR：在 *My Account → SSH Public Key* 追加专用 `aur_kenvoy.pub`，保留原有个人公钥，再验证 `ssh aur@aur.archlinux.org help`。无需网页建包；未登记公钥时不能把 Secret 存在当成 SSH 已可用。

## 验证边界

114 项渠道回归中 113 项通过、1 项因 Windows 符号链接权限跳过；真实只读 npm/WinGet 计划、8 个 Linux 包、签名、不可变包复用、APT 索引及 npm 封装运行的证据见 [0029](../changes/0029-channel-rollout-continuation.md#执行验证记录)。本机没有完整 RPM/Alpine/Arch 索引工具，未执行这些源的真实安装；也没有运行 GitHub 渠道发布、创建 WinGet PR 或推送 AUR。正式可用性仍须逐渠道安装验收，不能把 Secret 配置和本地包生成当成上线。
