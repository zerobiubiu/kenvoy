# 发布版本对齐：CLI 0.4.1 与 AUR 凭据清理

- 状态：已执行（版本、标签与文档已对齐；未推送、未发布）。
- 日期：2026-11-04。
- 影响范围：Kenvoy CLI 版本、`Cargo.lock`、版本断言、当前版本说明、版本日志与仓库 Secret。
- 前置记录：[0028 强制语言与集中译文](0028-mandatory-languages-and-schema.md)、[0029 多渠道发布配置续接](0029-channel-rollout-continuation.md)、[0030 退役 Chocolatey 与 AUR](0030-remove-chocolatey-and-aur.md)。
- 现状入口：[渠道进度](../development/channel-rollout-status.md)、[Git 与发布操作](../operations/releasing.md)。

## 背景

首次源码发布由 [release_common.require_tag](../../.github/scripts/release_common.py) 把关：发布新源码时要求版本 tag 的树与待发布提交的树**完全相同**。

实际状态是：`v0.4.0` 指向 `4e960afa`（树 `a689af29…`），此后 `dev` 又产生了 `.gitignore` 与渠道工程提交，树变为 `f1cbffad…`，再叠加 0030 后继续变化。`v0.4.0` 从未发布、也从未推送到远端（远端只有 `v0.1.0`、`v0.2.0`），但按仓库规则不得为通过门禁而移动既有标签，也不得放宽校验。

用户确认采用递增 PATCH 的方案：先提升版本，再在包含该版本的提交上打新标签。

## 方案

- CLI 由 0.4.0 升至 **0.4.1**，KEngine 保持 0.2.0；项目格式与最低 CLI **仍为 0.4.0**，因为那是配置文件契约，与产品版本是两套语义。
- `v0.4.1` 只打在包含 0.4.1 版本声明的提交上；`v0.4.0` 保持原状、永不推送，`v0.3.0` 同样作为未发布本地标签保留。
- 0.4.0 的产品内容随 0.4.1 一并交付；渠道工程变更从 0.4.0 的版本日志移入 0.4.1，避免把打标签之后的改动记到旧版本名下。
- 按用户授权删除仓库 Secret `AUR_SSH_PRIVATE_KEY`：AUR 已退役，该凭据无消费者；本地受限恢复副本保留，不做销毁。

## 实施

| 文件 | 改动 |
|---|---|
| [crates/kenvoy/Cargo.toml](../../crates/kenvoy/Cargo.toml) | 版本 0.4.0 → 0.4.1 |
| [Cargo.lock](../../Cargo.lock) | 同步 kenvoy 包版本，依赖清单不变 |
| [catalog.rs](../../crates/kenvoy-tests/tests/i18n_support/catalog.rs) | `framework_version` 断言随 CLI 版本改为 0.4.1；`project_format_version`、`min_cli_version` 仍断言 0.4.0 |
| [README.md](../../README.md)、[crates/kenvoy/README.md](../../crates/kenvoy/README.md) | 说明当前源码版本为 0.4.1、尚未发布 |
| [docs/README.md](../README.md) | 当前 dev 版本改为 0.4.1，并注明项目格式仍为 0.4.0 |
| [CHANGELOG.md](CHANGELOG.md) | 新增 0.4.1 小节，0.4.0 标注为未发布并并入本版 |
| [channel-rollout-status.md](../development/channel-rollout-status.md) | 记录版本决策、Secret 删除与推送顺序 |
| 仓库 Secret | 删除 `AUR_SSH_PRIVATE_KEY`；剩余 5 项渠道凭据加既有 `CARGO_TOKEN` |

## 执行验证记录

环境：Windows 10.0.26300 x64、Rust 1.98、uv Python 3.12、actionlint 1.7.12，全部使用 `--offline --locked`。

| 验证命令或场景 | 实际输出/结果 | 结论与限制 |
|---|---|---|
| `cargo check --workspace --offline --locked` | 编译通过，`Finished dev profile` | 锁文件与版本声明一致 |
| `cargo build -p kenvoy --offline --locked` | `Compiling kenvoy v0.4.1` | 产物版本确为 0.4.1 |
| `target/debug/kenvoy.exe --version` | `kenvoy 0.4.1` | 二进制与包版本一致 |
| `KENVOY_BINARY=... cargo test -p kenvoy-tests --offline --locked --test native_i18n omitted_languages` | `1 passed; 0 failed; 9 filtered out` | 构建产物 `framework_version` 随 CLI 版本更新，项目格式仍为 0.4.0 |
| `python -B -m unittest discover -s .github/scripts/channels/tests` | `Ran 112 tests in 13.943s`；`OK (skipped=1)` | 版本变更未影响渠道生成器 |
| `cargo run --package xtask --offline --locked -- check` | 架构、源码与文档均通过 | 新增文档链接可达 |

未推送 tag、未合并 main、未运行 GitHub Actions；`v0.4.1` 只是本地标签，验证范围仅限本机。

## 后续验证建议

合并前确认：`v0.4.1^{tree}` 与将推送的 `main` 树一致、远端已有 `v0.4.1`。推送顺序为先 `v0.4.1` 再 `main`，随后观察 `main 最新版本自动发布` 与自动触发的 `多渠道软件包发布`。tag 之后若再追加提交，需重新对齐版本与标签，不能沿用旧 tag。
