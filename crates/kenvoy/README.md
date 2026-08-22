# kenvoy

CLI toolkit built on [`kengine`](https://github.com/kenvoy/kenvoy).

## 安装

```bash
cargo install kenvoy
```

## 使用

```bash
kenvoy --help
kenvoy --version
kenvoy hello world
kenvoy info
```

## 作为库依赖

虽为 CLI crate，也可复用 `kengine`；workspace 内通过 `kengine = { workspace = true }` 统一版本。

## MSRV

Rust 1.98+, edition 2024.
