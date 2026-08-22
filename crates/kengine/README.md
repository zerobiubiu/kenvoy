# kengine

Core engine library for [`kenvoy`](https://github.com/kenvoy/kenvoy).

提供可复用的核心原语与运行时能力，被 `kenvoy` CLI 依赖。

## 用法

```toml
[dependencies]
kengine = "0.1"
```

```rust
use kengine::{hello, VERSION};

fn main() {
    println!("{}", hello("world"));
    println!("engine {}", VERSION);
}
```

## MSRV

Rust 1.98+, edition 2024.
