# 0020：许可证正文单一来源

状态：已执行；治理：不可变变更记录；日期：2026-11-03。影响范围：Workspace 与各成员 Cargo.toml 的许可证元数据、打包内容；前置：[0019](0019-crates-and-binary-release.md)；现行流程：[发布操作](../operations/releasing.md)。

## 背景与方案

0.2.0 发布时在 `crates/kengine/LICENSE` 与 `crates/kenvoy/LICENSE` 各放了一份根目录 LICENSE 的物理副本，以保证两个发布包各自携带许可证正文。核对后确认副本与根文件逐字节一致，但属于纯维护冗余。

Cargo 的 `[workspace.package]` 支持 `license-file` 继承，且成员继承的 `license-file` 路径相对 Workspace 根解析；打包时 Cargo 始终把声明的 license-file 文件放进 `.crate`，与 include 白名单无关。据此改为：

- 根 `Cargo.toml`：`license = "MIT"` 改为 `license-file = "LICENSE"`（两者互斥）。
- 四个成员（kengine、kenvoy、kenvoy-tests、xtask）统一改为 `license-file.workspace = true`。
- 删除两份 crate 目录 LICENSE 副本；仓库仅保留根目录一份。

两个 crate README 的许可表述（"MIT，见随包提供的 LICENSE"）不受影响。

## 排查结论：无其他类似入库重复

按内容哈希扫描全部非忽略文件（771 个），除上述两份 LICENSE 外，其余重复组均位于 gitignore 的运行产物（示例应用 `.kenvoy/`、`dist/` 镜像缓存），不属于源码副本。两份包级 README 内容各自独立，不是重复文件。

## 实施与验证

改动文件：`Cargo.toml`、`crates/kengine/Cargo.toml`、`crates/kenvoy/Cargo.toml`、`crates/kenvoy-tests/Cargo.toml`、`xtask/Cargo.toml`；删除 `crates/kengine/LICENSE`、`crates/kenvoy/LICENSE`。

- `cargo check --workspace --offline --locked` 退出 0。
- `cargo package -p kengine -p kenvoy --offline --allow-dirty` 均退出 0；解包后包内 `LICENSE` 与根 LICENSE 逐字节一致，包 metadata 为 `license-file = "LICENSE"`，`cargo package --list` 中 LICENSE/README 均在列。
- 已发布二进制分发的 workflow 归档步骤使用包目录内的 LICENSE，而包目录内容来自 `.crate`，其中必含 license-file，链路不受影响。
- `cargo run --package xtask --offline --locked -- docs` 退出 0，`markdown_files: 87`、`local_links: 880`、`status: 通过`（以实际提交时输出为准）。

## 边界与后续

- 已发布的 0.2.0 crate 与 GitHub 资产不重传：其包内许可证正文与根 LICENSE 内容完全一致，仅仓库布局变化。
- 下一次发布自然使用单一来源布局；无需为此递增产品版本号（未改变已发布程序行为）。

## 执行验证记录

- 执行环境：Windows x64 本机，Rust 1.98 工作区。
- 验证命令与输出：见上文"实施与验证"。
- 已知限制：`cargo package` 为打包级验证，不重新发布；跨架构运行行为与本次改动无关。
