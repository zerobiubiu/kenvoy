# kenvoy

`kenvoy` CLI + `kengine` 库，Rust 1.98 / edition 2024。

```
kenvoy/
├── crates/kengine  # 库
└── crates/kenvoy   # CLI (依赖 kengine)
```

```bash
cargo check --workspace
cargo test --workspace
cargo run -p kenvoy -- --help
cargo run -p kenvoy -- hello world
```

License: MIT
