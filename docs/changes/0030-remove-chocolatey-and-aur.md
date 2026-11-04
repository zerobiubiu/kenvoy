# 退役 Chocolatey 与 AUR 渠道

- 状态：已执行（实现与现行文档已移除；未发布、未推送）。
- 日期：2026-11-04。
- 影响范围：渠道工作流、渠道清单生成器、渠道回归、仓库规则与现行发布文档；不改变 CLI/KEngine 行为或版本。
- 前置记录：[0024 多渠道软件包发布](0024-package-channels.md)、[0029 多渠道发布配置续接](0029-channel-rollout-continuation.md)。
- 现状入口：[多渠道软件包发布](../operations/distribution-channels.md)、[渠道进度](../development/channel-rollout-status.md)。

## 背景

用户明确要求不再向 Chocolatey 发布，随后又要求 AUR 的包也不再发布。两个渠道此前都只有本地生成物和凭据配置：Chocolatey 从未推送 nupkg，AUR 从未登记公钥、从未创建 `kenvoy-bin`。因此本次是删除未上线实现，不涉及下架、撤销版本或迁移已发布包。

保留 npm、Homebrew、Scoop、WinGet 与 APT/RPM/Alpine/Arch 仓库。Arch 的 pacman 仓库与 AUR 的 `kenvoy-bin` 是两套不同产物，前者继续保留。

## 方案

- 直接删除实现与测试，不保留开关、占位 job 或“以后可能恢复”的分支，避免无消费者的代码继续漂移。
- 现行文档按当前渠道集合改写；0024、0029 属不可变变更记录，正文不改写，由本记录声明后续替代关系。
- 已存在的 `CHOCOLATEY_API_KEY` 从未配置，无需清理；`AUR_SSH_PRIVATE_KEY` 仍在仓库 Secret 中但已无消费者，删除 Secret 属对外破坏性操作，留待用户决定。
- 既不移动版本标签，也不因架构清理提升产品版本。

## 实施

| 文件 | 改动 |
|---|---|
| [channels.yml](../../.github/workflows/channels.yml) | 删除 `chocolatey` 与 `aur` 两个 job；其余 job 的主分支门禁、固定源码与签名流程不变 |
| [manifests.py](../../.github/scripts/channels/manifests.py) | 删除 `chocolatey_files`、`aur_files` 及其调用；脚本只生成 brew/scoop/winget，文档字符串与 CLI 描述同步 |
| [linux_packages.py](https://github.com/zerobiubiu/kenvoy/blob/098e24b615e1b602d4e871498d7790ea28e318a0/.github/scripts/channels/linux_packages.py) | 删除 Arch 包中指向已退役 AUR 包名的 `conflicts` 声明；pacman 仓库产物本身不变 |
| [test_manifests.py](../../.github/scripts/channels/tests/test_manifests.py) | 删除两渠道断言，渠道清单期望集合收敛为 5 个文件 |
| [distribution-channels.md](../operations/distribution-channels.md) | 范围、流程图、章节、Secret 表与配置步骤只保留现行渠道 |
| [channel-rollout-status.md](../development/channel-rollout-status.md) | 重写现状、Secret 在用清单、发布路径与版本门禁说明 |
| [README.md](../../README.md)、[AGENTS.md](../../AGENTS.md) | 分发说明与渠道规则同步，明确两渠道已退役且不恢复 |
| [CHANGELOG.md](CHANGELOG.md) | 0.4.0 的工程与分发条目记录退役 |
| 分类索引：[changes](README.md)、[verification](../verification/README.md)、[operations](../operations/README.md)、[文档中心](../README.md) | 补本记录入口与当前渠道范围 |

## 执行验证记录

环境：Windows 10.0.26300 x64、uv Python 3.12、actionlint 1.7.12。

| 验证命令或场景 | 实际输出/结果 | 结论与限制 |
|---|---|---|
| `uv run --no-project --python 3.12 python -B -m unittest discover -s .github/scripts/channels/tests` | `Ran 112 tests in 17.802s`；`OK (skipped=1)` | 相对 0029 减少 2 项（Chocolatey 架构选择、AUR 双架构）；Windows 符号链接用例跳过 |
| `grep -rniE 'chocolatey|choco ' .github/` | 无匹配 | 工作流与脚本不再残留两渠道实现 |
| `grep -rn 'kenvoy-bin' .github/` | 仅命中 `$RUNNER_TEMP/kenvoy-binaries` 目录名 | 该名称是二进制落盘目录，与 AUR 包名无关 |
| `actionlint .github/workflows/channels.yml` | 退出码 0，无诊断 | 删除 job 后工作流语法与表达式仍有效 |
| `cargo run --package xtask --offline --locked -- check` | 架构、源码与文档均通过；102 个 Markdown、1101 个本地链接 | 删除文档后链接与规模门禁未回退 |

本机未运行 GitHub Actions，未向任何渠道上传；删除 job 不影响已发布 Release 与 crates.io 版本。

## 后续验证建议

合并到 main 后首次运行渠道工作流时，确认 job 列表只剩 `prepare`、`npm`、`homebrew`、`scoop`、`winget`、`linux_repos`，且没有 Chocolatey/AUR 相关 Secret 缺失报错。渠道上线验收仍按渠道进度页逐项执行。
