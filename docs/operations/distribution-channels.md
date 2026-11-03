# 多渠道软件包发布

状态：参考；治理：现状文档。适用范围：Kenvoy CLI 二进制分发到 npm、Homebrew、APT、RPM、Alpine、Arch、WinGet、Chocolatey、Scoop。产品源包发布仍见 [Git 与发布操作](releasing.md)；本轮实施见 [0024](../changes/0024-package-channels.md)。

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
  C --> J[APT/RPM/Alpine/Arch Pages 仓库]
```

渠道工作流为 [.github/workflows/channels.yml](../../.github/workflows/channels.yml)：由 `main 最新版本自动发布` 成功结束触发，也支持 `workflow_dispatch` 指定已发布版本补发。它只消费已存在的 GitHub Release 资产，不重新编译、不改动 crate 或 Release。每个渠道独立 job，单渠道失败不撤销已经完成的 Release；修复后重跑同一工作流即可，已存在的内容按各自规则跳过或幂等更新。

前置条件：版本 tag 已推送；Release 有六个 ZIP 且带 GitHub 官方 digest。缺任一归档时在 `prepare` 阶段失败，不进入各渠道。0.2.0 及更早的 Release 使用旧的三元组文件名，不在本工作流兼容范围内；渠道发布从采用 `kenvoy-{version}-{platform}-{arch}.zip` 的下一个版本开始。

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

主包通过 `optionalDependencies` 依赖六个平台包；平台包声明 `os`/`cpu`，内含真实 `bin/kenvoy(.exe)`。`npm publish --access public --provenance` 使用 `NPM_TOKEN`；已存在同版本时跳过，不覆盖。

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

## Chocolatey

`kenvoy.nuspec` + `tools/chocolateyInstall.ps1`，安装脚本按 `PROCESSOR_ARCHITECTURE` 选择 x64/ARM64 归档、校验 checksum、解压到 tools 并 `Install-BinFile`。使用 Windows runner 的 `choco pack/push` 与 `CHOCOLATEY_API_KEY`。社区源有审核延迟，推送成功不等于立即可安装。

## AUR

生成 `kenvoy-bin` 的 `PKGBUILD` 与 `.SRCINFO`，`arch=(x86_64 aarch64)`，直接安装 Release 二进制。使用 `AUR_SSH_PRIVATE_KEY` 推送到 `ssh://aur@aur.archlinux.org/kenvoy-bin.git`。

**外部前置**：AUR 账号需在 *My Account → SSH Public Key* 登记 CI 使用的公钥。AUR 没有网页建包流程：CI 首次 `git clone ssh://aur@aur.archlinux.org/kenvoy-bin.git` 后推送 `HEAD:master` 即自动创建 `kenvoy-bin`；只接受 `master` 分支，且必须同时包含 `PKGBUILD` 与 `.SRCINFO`。

## APT / RPM / Alpine / Arch 仓库

托管在公共仓库 `zerobiubiu/kenvoy-packages` 的 `gh-pages` 分支，通过 raw.githubusercontent 提供文件（该账号 Pages 绑定的自定义域 DNS 不在 GitHub，暂不使用 Pages 地址）。四个仓库共用一份站点，按版本累加，旧版本文件不删除。

基础地址：`https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages`。raw 不提供目录浏览，但 `apt/dnf/apk/pacman` 只需按固定路径取索引与包文件，因此可直接使用。

| 仓库 | 路径 | 索引与签名 |
|---|---|---|
| APT | `/apt` | `dists/stable` + `pool/main/k/kenvoy`；`Packages.gz`、`Release`，GPG 清签 `InRelease` 与 `Release.gpg` |
| RPM | `/rpm` | `createrepo_c` 生成 `repodata`，`repomd.xml` GPG 分离签名 |
| Alpine | `/alpine` | `APKINDEX.tar.gz`，使用 APK RSA 密钥签名；公钥随站点发布 |
| Arch | `/arch` | `repo-add` 生成 `kenvoy.db.tar.gz`，GPG 签名 |

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

# Arch
# /etc/pacman.conf:
# [kenvoy]
# Server = https://raw.githubusercontent.com/zerobiubiu/kenvoy-packages/gh-pages/arch/$arch
```

`kenvoy.repo`、`key.asc`、`RPM-GPG-KEY-kenvoy`、`kenvoy.rsa.pub` 由仓库发布脚本随索引生成。

## 所需仓库 Secret

| Secret | 用途 | 生成方式 |
|---|---|---|
| `NPM_TOKEN` | npm 发布 7 个包 | npm 账号 Automation token |
| `PACKAGES_TOKEN` | 推送 tap/bucket/Pages 仓库 | GitHub PAT（`repo`，对三个辅助仓库有写权限） |
| `WINGET_TOKEN` | fork 并 PR 到 microsoft/winget-pkgs | GitHub PAT（`public_repo`） |
| `CHOCOLATEY_API_KEY` | 推送 Chocolatey 包 | community.chocolatey.org API Key |
| `AUR_SSH_PRIVATE_KEY` | 推送 AUR | AUR 账号 SSH 私钥（OpenSSH 格式） |
| `GPG_PRIVATE_KEY` | APT/RPM/Arch 仓库签名 | 导出私钥后 base64 |
| `GPG_PASSPHRASE` | 上述密钥口令（无口令可留空） | — |
| `APK_SIGNING_KEY` | APK 包与索引签名 | RSA 私钥 PEM 后 base64 |

`prepare` 之后各 job 在缺 Secret 时明确失败并指出名称，不伪造成功。所有密钥只在对应 job 注入，不打印、不写入制品。

### 配置步骤（Windows / Git Bash）

凭据只在本机生成或复制，用 `gh secret set` 直接写入仓库，不经过聊天、源码或命令输出。

```bash
# 1. npm：npmjs.com → Access Tokens → Generate New Token → Automation
gh secret set NPM_TOKEN --repo zerobiubiu/kenvoy

# 2. 辅助仓库：Classic PAT 勾选 repo；或 Fine-grained 授权三个辅助仓库 Contents: Read and write
gh secret set PACKAGES_TOKEN --repo zerobiubiu/kenvoy

# 3. WinGet：Classic PAT 勾选 public_repo（需要 fork 并向上游开 PR）
gh secret set WINGET_TOKEN --repo zerobiubiu/kenvoy

# 4. Chocolatey：community.chocolatey.org → Account → API Keys
gh secret set CHOCOLATEY_API_KEY --repo zerobiubiu/kenvoy
```

仓库签名与 AUR 使用专用密钥（不要复用个人主密钥）：

```bash
# GPG：仅签名用途，有效期按需；导出 base64 单行
gpg --batch --quick-generate-key "Kenvoy Release <release@users.noreply.github.com>" rsa4096 sign 5y
KEY=$(gpg --list-secret-keys --with-colons | awk -F: '/^sec/{print $5; exit}')
gpg --armor --export-secret-keys "$KEY" | base64 -w0 > gpg-private.b64
gh secret set GPG_PRIVATE_KEY --repo zerobiubiu/kenvoy < gpg-private.b64
gh secret set GPG_PASSPHRASE --repo zerobiubiu/kenvoy   # 生成密钥时设置的口令值（建议设置）

# APK：RSA 私钥，CI 推导公钥并发布
openssl genrsa -out kenvoy.rsa 4096
base64 -w0 kenvoy.rsa > apk-signing.b64
gh secret set APK_SIGNING_KEY --repo zerobiubiu/kenvoy < apk-signing.b64

# AUR：专用 SSH 密钥，公钥登记到 AUR，私钥写入 Secret
ssh-keygen -t ed25519 -C "aur-kenvoy-ci" -f ~/.ssh/aur_kenvoy -N ""
gh secret set AUR_SSH_PRIVATE_KEY --repo zerobiubiu/kenvoy < ~/.ssh/aur_kenvoy
```

写入后不要保留临时文件中的私钥副本；`gpg-private.b64`、`apk-signing.b64`、`kenvoy.rsa` 用后删除。核对：

```bash
gh secret list --repo zerobiubiu/kenvoy
```

AUR 建包不需要网页操作：登录 aur.archlinux.org，在 *My Account → SSH Public Key* 粘贴 `~/.ssh/aur_kenvoy.pub`，确认 `ssh aur@aur.archlinux.org help` 可用即可；`kenvoy-bin` 在 CI 首次推送 `master` 时自动创建。

## 验证边界

本轮只做本地产物生成与静态检查，尚未运行 GitHub 上的渠道发布，也不代表各包管理器已完成审核（WinGet PR、Chocolatey 审核、AUR 首次建仓均为外部流程）。实际可用性以对应仓库/源的安装结果为证据，见 [0024](../changes/0024-package-channels.md) 的执行记录。
