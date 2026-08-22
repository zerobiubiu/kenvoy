# AGENTS.md — kenvoy

Rust 1.98 / edition 2024 / resolver 3。`crates/*` 自动发现；`kengine`(lib) → `kenvoy`(bin)。

```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
cargo package -p kengine --list --allow-dirty
cargo publish -p kengine --dry-run --allow-dirty  # 先发 kengine，再发 kenvoy
```

约束：LICENSE 为 MIT；`[workspace.dependencies]` 统一版本；不提交 `target/`。
