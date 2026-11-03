//! 原生 i18n 的真实 CLI、HTTP、SQLite、构建及无源码部署验收。

#[path = "i18n_support/catalog.rs"]
mod catalog;
#[path = "i18n_support/http.rs"]
mod http;
#[path = "i18n_support/reload.rs"]
mod reload;
#[path = "i18n_support/startup.rs"]
mod startup;
#[path = "i18n_support/mod.rs"]
mod support;
