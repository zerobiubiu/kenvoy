# 多渠道发布配置续接与安全门禁

- 状态：已执行（本地实现、已授权凭据配置与验证；未运行远端发布）。
- 日期：2026-11-04。
- 影响范围：渠道 CI、Python 分发脚本、仓库 Secret、专用签名/SSH 密钥及发布文档；不改变 CLI/KEngine 行为或版本。
- 前置记录：[0024 多渠道软件包发布](0024-package-channels.md)、[0021 分支与发布规则](0021-dev-main-release-rules.md)、[0023 标签规则](0023-version-tag-rules.md)。
- 现状入口：[渠道进度](../development/channel-rollout-status.md)、[发布操作](../operations/distribution-channels.md)。

## 背景与授权边界

历史暂停点是：生成脚本及三个辅助仓库已经存在，只有 CARGO_TOKEN；八个渠道 Secret 未配置，main 仍是旧工作流，尚未真实上传。此次先核对历史记录，再通过 GitHub API、npm registry、本机工具及账号登录页确认现状，未将旧记录当作实时状态。

用户明确授权复用当前 GitHub 登录凭据配置 PACKAGES_TOKEN/WINGET_TOKEN，并生成专用签名与 AUR 密钥、写入 Secret、保留受限本地恢复副本。授权不包括推送 dev/main/tag、合并、发布包、创建 WinGet fork/PR 或 AUR 首次推送。

本次保留用户既有 `.gitignore` 提交；CLI 保持 0.4.0，KEngine 保持 0.2.0。纯工程配置不机械递增产品版本，也不移动已存在的附注标签。

## 实施

### 凭据、账号与恢复副本

已写入六项渠道 Secret：PACKAGES_TOKEN、WINGET_TOKEN、GPG_PRIVATE_KEY、GPG_PASSPHRASE、APK_SIGNING_KEY、AUR_SSH_PRIVATE_KEY。原 CARGO_TOKEN 未修改；GitHub 只能回读 Secret 名称和更新时间，不能回读值。

三个辅助仓库均为公开仓库且当前账号有管理/推送权限；kenvoy-packages 的 gh-pages 仍只有初始入口，没有虚构已经发布的软件包。没有向这些仓库推送。

专用密钥包括 RSA 4096 OpenPGP 签名密钥、RSA 4096 PKCS#1 APK 密钥及 Ed25519 AUR 密钥。GPG 指纹为 `D03C849C5A87865D5251A61715A7EFB37DFFE7B8`；AUR 指纹为 `SHA256:f6bwtvLP1vxHBDV8d63pjOgP4UHW4hCv2MFOKAopkXY`。恢复副本在仓库外，仅当前 Windows 用户和 SYSTEM 可访问；GPG 导出加密保存，口令由 Windows CurrentUser DPAPI 保护，迁机前必须安全导出口令。保留撤销证书，不保留临时明文口令文件。

npm、Chocolatey、AUR 登录页均已打开并交回用户；尚未收到登录完成确认。因此 NPM_TOKEN、CHOCOLATEY_API_KEY 及 AUR 账号公钥登记仍未完成，不擅自操作已交回的浏览器，也不在聊天中收取密码、验证码或 Token。

### 发布失败保护

- 渠道工作流先限制 main、来源分支和来源仓库，固定后续检出的源码 SHA，并设置确定的 SOURCE_DATE_EPOCH。
- 资产解析拒绝 draft、prerelease、错误 tag 和未发布 Release；下载与缓存均比对官方 SHA-256，不覆盖损坏文件。
- npm 发布器默认只读，精确区分 404 与鉴权/网络/限流/服务错误；校验全部包及原始 LICENSE，先确认所有版本状态，再发布平台包和主包。只有显式 `--publish` 且处于 main 才写入；既有版本跳过，失败不盲目重试。
- npm 使用官方 OIDC/Token 选择，不再把 NPM_TOKEN 当作唯一可能的鉴权方式。首发前仍需合法创建七个包并配置可信发布；旧 Automation token 指引已删除。
- WinGet 发布器默认只读，验证目标账号与 fork 上游，支持已有 PR 跳过和部分清单提交恢复；不将 API 错误吞成“资源不存在”，不强推分支。
- Chocolatey 的 Python、pack、push 都显式检查退出码，避免 PowerShell 后续命令掩盖失败。

### Linux 包与仓库历史

nfpm 固定版本并校验下载摘要，RPM 使用专用私钥和 16 位长 key ID 签署，口令仅走环境。Arch 包先签名，再将包与 `.sig` 一起交给索引生成器；仓库元数据另做签名。

生成前必须成功读取既有 gh-pages。已发布的同版本包及 Arch 签名直接复用；若合并遇到同名不同内容，则失败，不覆盖。新索引保留历史包；Arch 的 db/files 及签名别名转换为真实文件，避免 GitHub raw 返回符号链接文本。发布前检查签名，最终普通快进推送，不再 orphan/force 覆盖分支历史。

### 改动文件

| 文件 | 改动 |
|---|---|
| [.github/workflows/channels.yml](../../.github/workflows/channels.yml) | 主分支/来源门禁、固定源码、发布器接线、退出码、密钥隔离、签名与历史保留 |
| [assets.py](../../.github/scripts/channels/assets.py) | 正式 Release 与下载/缓存摘要检查 |
| [npm_packages.py](../../.github/scripts/channels/npm_packages.py)、[publish_npm.py](../../.github/scripts/channels/publish_npm.py) | 原始 LICENSE 交付、只读计划及显式发布 |
| [publish_winget.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/publish_winget.py) | 固定上游、fork/PR 幂等与错误处理 |
| [linux_packages.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/linux_packages.py)、[build_linux_packages.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/build_linux_packages.py)、[linux_repo.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/linux_repo.py) | RPM 签名配置、既有包复用、Arch 包签名进入索引 |
| [test_npm_packages.py](../../.github/scripts/channels/tests/test_npm_packages.py)、[test_publish_npm.py](../../.github/scripts/channels/tests/test_publish_npm.py) | 缺许可证拒绝、发布顺序、已有版本与错误边界 |
| [test_publish_winget.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/tests/test_publish_winget.py) | 真实临时 Git 仓库中的恢复、越界分支与 API 失败 |
| [test_release_assets.py](../../.github/scripts/channels/tests/test_release_assets.py)、[test_build_linux_packages.py](https://github.com/zerobiubiu/kenvoy/blob/511455e4c8c21a6f4fe66f2d63cf222c1979bd9d/.github/scripts/channels/tests/test_build_linux_packages.py) | 非正式资产、坏摘要、缓存保护、包与签名不变性 |
| [渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md)、[版本日志](CHANGELOG.md)及分类索引 | 当前配置、账号阻断、安装与恢复边界、证据入口 |

## 发布前仍须处理

1. npm 与 Chocolatey 凭据、AUR 公钥登记及真实账号权限仍待用户完成；Secret 存在不等于该渠道已能发布。
2. 七个 npm 包的 registry 查询均为 404，可信发布绑定尚未建立。依据 [npm token 文档](https://docs.npmjs.com/about-access-tokens/)和[可信发布文档](https://docs.npmjs.com/trusted-publishers/)，改用 Granular token/交互 2FA 首发，并在实际发布窗口建立允许 `npm publish` 的 OIDC 绑定。
3. 现行 [require_tag](../../.github/scripts/release_common.py) 在发布新源码时比较整个 Git 树，不只产品源码。实际 `v0.4.0^{tree}` 为 `a689af29e72982ead9ce7da187b1dc4f59b30f25`，本轮开始时 HEAD 树为 `4b47c844f77d1d2b4d171445965f00215e22210f`，已不相等；本次工程提交也不会使其相等。正式发布时必须显式确定版本安排或另行批准规则调整，不能自动移动标签或声称现有 0.4.0 已具备发布前提。
4. main 尚未包含渠道工作流。没有推送、合并或 workflow dispatch；0.2.0 的旧三元组归档只能作本地验证输入，不能让自动渠道流程将其补发。
5. 首发后须分别验证真实安装及版本、RPM/APK/Arch 完整验签、WinGet PR 审核、Chocolatey 审核和 AUR 首次建包。单渠道成功不能代表其余渠道成功。

## 执行验证记录

环境：Windows 10.0.26300 x64、Git for Windows GPG/OpenSSL、uv Python 3.12、npm 12.2.0、nfpm 2.43.0、actionlint 1.7.12；APT 索引使用既有 WSL Ubuntu 24.04 的 apt-ftparchive。临时项目与打包目录独立，不操作用户业务数据库。

| 验证命令或场景 | 实际输出/结果 | 结论与限制 |
|---|---|---|
| `gh repo view`、辅助仓库 API、`gh secret list --repo zerobiubiu/kenvoy --json name,updatedAt` | 主仓库与三个辅助仓库权限通过；六项渠道 Secret 加既有 CARGO_TOKEN；无 npm/Chocolatey Secret | 证明配置及权限元数据，不证明外部渠道已上线 |
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/channels/tests -v` | `Ran 114 tests in 16.384s`；`OK (skipped=1)` | 113 通过；Windows 符号链接用例跳过 |
| `actionlint.exe .github/workflows/channels.yml` | 退出码 0，无诊断 | 工作流语法与表达式检查通过 |
| 抽取真实 prepare 首个 run 脚本，由 Git Bash 执行四组事件环境 | 本仓库 main：0；dev、tag、外仓 main：均 1 | 主分支与来源拒绝门禁实际执行 |
| `publish_npm.py --packages <临时七包目录>`，不传 `--publish` | 全部版本返回未发布，平台包在前、主包在后；只读模式不执行发布 | 真实 registry 查询；没有上传 |
| `publish_winget.py --manifests <三清单目录> --version 0.2.0 --owner zerobiubiu`，不传 `--publish` | 生成面向 microsoft/winget-pkgs master 的只读 fork/branch/PR 计划 | 无 fork、分支或 PR 写入 |
| `npm_packages.py`、`manifests.py`、`linux_packages.py` 处理六个真实旧版 ZIP | 7 个 npm 包/21 个文件、9 份渠道清单、8 份 nfpm 配置及 PKGBUILD | ZIP 分别与当时 GitHub 官方 digest 一致；仅本地使用 |
| `npm pack --dry-run --json --ignore-scripts` | 实际列表含 LICENSE、bin/kenvoy.exe、package.json | npm 的 files 白名单未漏掉 LICENSE |
| `node <隔离 node_modules>/kenvoy/bin/kenvoy.js --version` | `kenvoy 0.2.0`，退出码 0 | 真实归档中的 Windows 二进制经 npm shim 运行，不是伪造 fixture |
| `build_linux_packages.py` 调用真实 nfpm | amd64/arm64 DEB、x86_64/aarch64 RPM、APK、Arch 共 8 包生成成功 | RPM/APK 使用签名配置；不等同于已通过各发行版安装验签 |
| GPG 实签/验签、OpenSSL RSA 实签/验签、SSH 公钥派生、DPAPI 往返 | GPG 退出码 0；RSA `Verified OK`；SSH 公钥一致；DPAPI 解密一致 | 验证专用密钥可用，未输出私钥或口令 |
| 真实包先进入旧仓库布局，再用不存在的 nfpm 路径运行包构建助手 | 8 包均输出“复用已发布包”；8 包加 2 个 Arch 签名逐字节一致 | 同版本重跑不依赖重打包、不改写包与签名 |
| WSL `python3 linux_repo.py --version 0.2.0 --packages <两 DEB> --repo <临时目录>`，随后 GPG 清签/验签 | `仓库索引完成：0.2.0，DEB 2、RPM 0、APK 0、Arch 0`；签名与验签退出码 0 | 真实 APT 工具路径通过；未安装到系统 |
| `git rev-parse 'v0.4.0^{tree}' 'HEAD^{tree}'` | 返回上述两个不同树哈希 | 明确记录源码发布门禁，不修改其规则 |
| `cargo run --package xtask --offline --locked -- check` | 架构、源码与文档均“通过”；427 个自有源码文件，最大 449 行；101 个 Markdown、1076 个本地链接 | 沿用本仓库工程入口；未改动产品源码，不重复运行产品测试套件 |
| `assets.py --version 0.2.0 --repository zerobiubiu/kenvoy --output <新路径>` | 退出码 1：`Release 缺少 kenvoy-0.2.0-linux-ARM64.zip`；没有生成清单 | 真实 API 返回的旧版归档命名按契约拒绝，未回溯上传 |
| `cargo run --package xtask --offline --locked -- version-tags --apply` | 新增 0、保留 5；未推送、未移动既有标签 | 只执行既定的本地标签维护，不改变版本安排 |

下载工具在执行前核对 GitHub 官方 SHA-256：actionlint Windows ZIP 为 `6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9`，nfpm Windows ZIP 为 `e3551b135b37d4a19521db4c549a2c81a214d819ee09623ac092c89f3324a854`。工作流内 nfpm Linux ZIP 固定为 `a80d5f724ed70b192ffa8a2bde469c013cef559c8afa3441eb51dd9a918beb6b`。

本地公共证据位于被忽略的 `artifacts/channel-rollout/`；临时密钥环、独占 GPG agent 与一次性探针在验证后清理，受限恢复副本保留。没有运行远端发布、完整 RPM/Alpine/Arch 真实索引与安装，也未将静态检查或模拟边界测试充当这些步骤的证据。后续逐渠道验收建议与未完成前置条件见上节和渠道进度页。
