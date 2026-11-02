# 0022：补齐根提交声明的 v0.1.0 标签

状态：已执行；治理：不可变变更记录；日期：2026-11-03。影响范围：本地版本标签维护；前置：[0021](0021-dev-main-release-rules.md)；现行规则：[Git 与发布操作](../operations/releasing.md)。本轮仅本地操作，不推送、不发布。

## 背景与来源

用户指出 v0.1.0 标签缺失。0021 将根提交声明的 manifest 版本仅作为比较基线，且拒绝根上的其他版本标签，导致自动补标签漏掉 0.1.0。

[已发布 kenvoy 0.1.0 的 .cargo_vcs_info.json](https://docs.rs/crate/kenvoy/0.1.0/source/.cargo_vcs_info.json) 明确记录源提交为 `7d3b5c2860d6e3c64948612a53d0df74ddf47883`；该提交的 Kenvoy manifest 同样声明 `version = "0.1.0"`。因此 v0.1.0 应指向这个根，而不是后来仍沿用同一版本号的其他提交。

## 修正

- 根提交声明的非 0.0.0 产品版本纳入首次版本候选；初始化 v0.0.0 与真实版本标签可共存，不增加或复制历史节点。
- 取消“根只能保留 v0.0.0”的限制，仍要求其他版本标签与目标提交 manifest 相符，保留不覆盖、不移动、不推送的保护。
- 增加初始化标记与根版本共存、重复运行保持标签对象不变的回归；原有合并来源、未提交版本隔离、冲突与浅历史检查保留。
- 自动命令实际新增附注标签 `v0.1.0 → 7d3b5c2860d6e3c64948612a53d0df74ddf47883`，保留既有 v0.0.0、v0.2.0。

改动：`xtask/src/commands/version_tags/{history,plan,mod,tests}.rs`、`AGENTS.md`、发布操作、版本日志与文档索引。0021 的历史正文不改写，本记录替代其中关于根版本仅作比较基线的规则。

## 执行验证记录

- 环境：Windows x64、Rust 1.98，本地 dev 分支。
- `cargo fmt --package xtask` 与 `cargo test --package xtask --offline --locked commands::version_tags::tests`：退出 0，9 项通过。
- `cargo run --package xtask --offline --locked -- version-tags --apply`：退出 0；实际输出新增 1、保留 2，目标为根提交，未推送、未移动既有标签。
- 本轮不递增产品版本，不重新构建产品或上传源包。后续提交钩子应保留三个标签，用户授权推送后再逐个上传本地标签。
- `cargo run --package xtask --offline --locked -- docs`：退出 0，91 个 Markdown、929 个本地链接通过。
