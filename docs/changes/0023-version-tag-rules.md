# 0023：版本标签规则定稿与初始化标签退役

状态：已执行；治理：不可变变更记录；日期：2026-11-03。影响范围：Git 标签规则、自动维护钩子及初始化特例；前置：[0021](0021-dev-main-release-rules.md)、[0022](0022-root-version-tag.md)；现行规则：[Git 与发布操作](../operations/releasing.md)。

## 背景与授权

用户要求明确并持续执行版本标签规则：只要已提交的版本变化就在对应提交打 tag，发现缺失及时补齐；删除 v0.0.0，并将本仓库分支和保留的版本标签推送 GitHub。此次推送授权不包含 dev→main 合并或强推。

## 实施

- 标签只依据已提交 Kenvoy CLI manifest 的真实版本。新版本在首次出现的提交创建附注 tag，补漏不挂到当前 HEAD；未提交/暂存版本和同版本普通改动不产生新标签。
- 删除自动创建和特殊接受 `v0.0.0 init` 的逻辑，以及发布计划中的人工初始化特例。根版本 v0.1.0 正常保留，不改变根提交或历史；已有合法发布标签仍不移动、不覆盖。
- 本地 v0.0.0 已删除，远端初始检查没有该标签。真实维护命令运行后仍只保留 v0.1.0、v0.2.0，没有重新生成初始化标签。
- `post-commit`、`post-checkout`、`post-merge`、`pre-push` 共用 `.githooks/maintain-version-tags`：在 dev 提交后、切回 dev、合并后和推送前自动核对补齐。后置钩子失败不伪称 Git 操作回滚；推送前失败则阻止推送。
- 钩子不自动联网推送，也不扩大用户给定的 refspec；版本标签仍需在授权范围内逐个推送。只读任务不写引用。

改动文件：`AGENTS.md`、`.githooks/`、`xtask/src/commands/version_tags/{history,plan,mod,tests}.rs`、`.github/scripts/{release_common,release_plan}.py`、发布与工程文档、CHANGELOG 及文档索引。产品版本未递增。

## 执行验证记录

环境：Windows x64、Rust 1.98、Python 3.12.15、本地 dev。

- `cargo fmt --package xtask`、`cargo test --package xtask --offline --locked commands::version_tags::tests`：退出 0，10 项通过。包括历史标签缺失后补到原版本提交、既有标签保持、拒绝无对应版本的初始化标签。
- `cargo run --package xtask --offline --locked -- version-tags --apply`：退出 0，新增 0、保留 2；v0.1.0 指向 `7d3b5c2…`，v0.2.0 指向 `d4056cfe…`，不生成 v0.0.0。
- Python 发布策略回归：退出 0，10 项通过；移除人工初始化特例后，main 当前版本门禁与已发布跳过行为保持。
- `git hook run post-checkout -- <当前提交> <当前提交> 1`、`git hook run post-merge -- 0`：退出 0，两次都实际调用标签维护，新增 0、保留 2；这是直接执行钩子，并未切换或合并分支。
- 本机工具 shell 无 `chmod` 命令，改用 Git 钩子入口直接验收成功；入库时使用 Git `--chmod=+x` 记录可执行模式，LF 继续由 `.gitattributes` 保证。
- `cargo fmt --package xtask -- --check` 与 `cargo run --package xtask --offline --locked -- check`：退出 0；四成员、依赖方向与规模门禁通过，最大 449 行；92 个 Markdown、938 个本地链接通过。

## 推送边界与后续

本轮按用户授权逐个推送保留的版本 tag，并正常推送 dev/main；不使用 `--mirror`、`--force` 或批量 `--tags`，不合并两分支、不重发产品。若远端引用发生冲突应停止，不覆盖远端修改。最终推送结果以本次 Git 命令回执为准，不在提交前预写成功。

0023 取代 0021—0022 中保留人工初始化 v0.0.0 标记的规则，历史正文不回写。新克隆仍须启用本仓库 hooksPath；其他已有克隆若残留旧 v0.0.0，应按用户授权删除，不能把冲突标签静默挪到其他提交。
