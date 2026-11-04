# 退役 Linux 软件源与 WinGet，准备 CLI 0.4.2

- 状态：已执行（本地实现、获授权的 Secret 删除与验证；不执行远端发布）。
- 日期：2026-11-05。
- 影响范围：APT、RPM、Alpine、Arch、WinGet 发布路径、专用脚本/测试、CLI 版本及现行文档。
- 前置记录：[0029 渠道续接](0029-channel-rollout-continuation.md)、[0030 退役 Chocolatey 与 AUR](0030-remove-chocolatey-and-aur.md)、[0031 版本对齐](0031-release-version-alignment.md)。
- 现状入口：[渠道操作](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md)。

## 背景与授权

用户进一步取消 APT、RPM、Alpine、Arch、WinGet，只保留 npm、Homebrew、Scoop；crates.io 与 GitHub Release 主发布流程不变。随后明确同意删除四项专用仓库 Secret，并递增 PATCH 到 0.4.2、维护新的本地标签。

本轮不推送、不合并、不 dispatch 工作流、不上传软件包。旧 `kenvoy-packages` 远端仓库与仓库外的密钥恢复副本保留；不会因为取消软件源就删除用户文件或卸载本机工具。

## 实施与边界

### 1. 完整移除五类发布源

- 删除 `channels.yml` 中的 `winget`、`linux_repos` 两个 job，剩余 `prepare`、`npm`、`homebrew`、`scoop`。一并删除只有 Linux 包构建使用的 `source_date_epoch` 输出和计算。
- 删除四个专用脚本及四个测试模块：`linux_packages.py`、`build_linux_packages.py`、`linux_repo.py`、`publish_winget.py` 和对应 `test_*`。
- 从共用 `manifests.py` 删除 WinGet 三清单生成、专用常量和调用，不保留禁用开关、空实现或兼容别名；只生成 Homebrew/Scoop 两份清单。
- 删除退役测试及纯文件复制回声断言，保留资产校验、平台 URL/摘要映射、路径边界和发布失败保护的消费者行为验证。

取消软件源不等于取消平台支持：Windows、Linux、macOS 的六目标 ZIP、npm 六个平台包、Homebrew 的 Linux/macOS 分支，以及 Scoop 的 x64/ARM64 支持继续保留。没有修改六目标分发配置或 crates.io 发布器。

### 2. 仅清理已获授权的凭据

实际删除 `WINGET_TOKEN`、`GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`APK_SIGNING_KEY`。删除后的 GitHub Secret 列表仅剩 `CARGO_TOKEN`、`PACKAGES_TOKEN`；前者用于 crates.io，后者仍供 Homebrew/Scoop 共用。不撤销用户 GitHub 登录令牌，不改动本地恢复副本。

`NPM_TOKEN` 仍未配置。有效 OIDC 或首发授权是 npm 的独立前置，本轮没有创建包、配置账号或声称 npm 已可发布。

### 3. 版本、项目格式与测试口径

CLI 由 0.4.1 升为 0.4.2，锁文件同步，KEngine 保持 0.2.0。最终版本提交沿用既有钩子维护本地附注标签 `v0.4.2`；旧标签不移动，完整 Git 树一致性门禁不放宽。

实测并区分两个事实：

- 既有项目显式声明 `min_cli_version = "0.4.0"`、`format_version = "0.4.0"`，仍可由 CLI 0.4.2 构建和运行。
- create 的既有实现取创建时 CLI 版本作为新项目的 `min_cli_version`，因此本轮生成 0.4.2，项目格式仍为 0.4.0。没有修改生成策略。

第一次一次性冒烟错误地把新建项目的最低 CLI 预期为 0.4.0，断言失败；读取实际清单和 `create.rs` 后修正探针预期，没有为迎合探针改变产品行为。0031 的“最低 CLI 仍为 0.4.0”只适合描述兼容下限，不能当作 create 的固定输出；现状文档已纠正。

删除测试中硬编码当前 `framework_version` 和 create 最低 CLI 的过时断言，不重新钉死到 0.4.2；版本输出与新建项目声明由本轮真实 CLI 冒烟核对，既有兼容性、译文与 HTTP 行为断言保留。

### 4. 文档与历史

README、AGENTS.md、版本日志、配置/CLI 契约、渠道操作与进度页只描述当前有效渠道。新增本记录，并补齐文档中心、变更与验证索引。

0029、0030 的历史实施说明和验证结论不改，只将被删源码的失效相对链接定位到原始提交 `511455e4c8c21a6f4fe66f2d63cf222c1979bd9d`、`098e24b615e1b602d4e871498d7790ea28e318a0`。对应 GitHub 固定链接在提交尚未推送时可能不可达；可在本地通过 `git show 提交:路径` 追溯，不把本地文档门禁当成这些外链已上线的证明。

## 改动文件

| 范围 | 文件与动作 |
|---|---|
| 工作流与共用生成器 | [.github/workflows/channels.yml](../../.github/workflows/channels.yml)、[manifests.py](../../.github/scripts/channels/manifests.py) |
| 删除的专用模块 | `.github/scripts/channels/{linux_packages,build_linux_packages,linux_repo,publish_winget}.py` 及 `tests/` 中四个对应模块 |
| 保留渠道测试 | [test_manifests.py](../../.github/scripts/channels/tests/test_manifests.py)，交付集合收敛为两份清单 |
| 版本与受影响测试 | [CLI Cargo.toml](../../crates/kenvoy/Cargo.toml)、[Cargo.lock](../../Cargo.lock)、[catalog.rs](../../crates/kenvoy-tests/tests/i18n_support/catalog.rs)、[i18n_cli.rs](../../crates/kenvoy/tests/i18n_cli.rs) |
| 现状与规则 | [根 README](../../README.md)、[CLI README](../../crates/kenvoy/README.md)、[AGENTS.md](../../AGENTS.md)、[配置契约](../contracts/project-configuration.md)、[CLI 操作](../operations/cli-design.md)、渠道操作/进度、版本日志与索引 |
| 历史源链接 | 0029、0030 中被删除文件的链接改为固定提交地址，历史事实不重写 |

## 执行验证记录

环境：Windows x64；沿用 uv Python 3.12、Cargo 离线锁定构建、已有 actionlint 1.7.12。所有生成和 CLI 应用验证使用自有临时目录，未使用用户业务库。

| 实际执行命令或场景 | 实际结果 | 证据边界 |
|---|---|---|
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/channels/tests -v` | `Ran 54 tests in 2.316s`，`OK` | 保留渠道全部通过；退役模块的测试随实现删除 |
| 删除前后分别运行 `manifests.py`、`npm_packages.py` | 删除前 5 份清单，删除后 2 份；两份保留清单逐字节一致；7 个 npm 包、21 个文件全部逐字节一致 | 使用六个已核对官方 SHA-256 的真实 0.2.0 ZIP，仅做本地生成，没有补发旧版 |
| 隔离 node_modules 内运行真实 npm shim `--version` | `kenvoy 0.2.0`，退出码 0 | 实际执行旧版归档内的 Windows 二进制，不是 mock |
| actionlint 检查 `channels.yml`，另用 Bun YAML 解析最终 job 图 | actionlint 退出码 0；仅 prepare/npm/homebrew/scoop，main 自动触发入口保留，无专用 Secret/时间戳残留 | 未运行远端工作流 |
| 目标源码与工作流引用扫描 | `.github`、`xtask`、`crates` 不再匹配 WinGet/Linux 软件源模块、专用 Secret 或 nfpm/索引工具引用 | 历史说明中的名称不是现行消费者 |
| `cargo build -p kenvoy --offline --locked` 后运行 `target/debug/kenvoy.exe --version` | 编译成功；`kenvoy 0.4.2` | 本机目标编译，不代替六平台构建 |
| 真实 CLI `create` → `check` → `build` | 全部退出 0；产物 framework_version=0.4.2，project_format_version=0.4.0，新建 min_cli_version=0.4.2；未创建业务 SQLite | 验证版本与项目格式分离，原始错误预期已如实记录 |
| `KENVOY_BINARY=... cargo test -p kenvoy-tests --offline --locked --test native_i18n omitted_languages` | 1 passed，0 failed，9 filtered out | 显式最低 CLI 0.4.0 的项目通过构建及实际 HTTP 场景 |
| `cargo test -p kenvoy --offline --locked --test i18n_cli` | 14 passed | 受影响 CLI 契约套件通过，不重钉过时的当前版本常量 |
| `cargo run --package xtask --offline --locked -- check` | 架构、源码与文档通过；104 个 Markdown、1127 个本地链接 | 本地门禁不检查固定提交外链是否已推送 |
| 删除四项 Secret 后读取名称 | 仅 CARGO_TOKEN、PACKAGES_TOKEN | 验证删除结果，不读取 Secret 值 |

本地公共证据保存于被忽略的 `artifacts/channel-retirement/evidence.json`；一次性项目和包目录在完成验证后清理。

## 后续验证建议

用户正式发布前先完成 npm 授权，再确认拟推送的 main 树与 `v0.4.2^{tree}` 一致，并先推送所需版本标签、再推送 main。观察主发布成功后仅触发三个保留渠道，分别安装核对版本。没有远端执行证据前，不把配置或本地生成写成发布成功。
