//! 真实 CLI、交付链、SQLite 与运行生命周期的跨层回归；不使用用户数据库。

#[path = "migration_support/adoption.rs"]
mod adoption;
#[path = "migration_support/catalog.rs"]
mod catalog;
#[path = "migration_support/runtime.rs"]
mod runtime;
#[path = "migration_support/mod.rs"]
mod support;
#[path = "migration_support/transactions.rs"]
mod transactions;
