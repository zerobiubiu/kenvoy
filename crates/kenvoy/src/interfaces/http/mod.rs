//! HTTP 适配层只消费已验证模型；请求期间不解析源码、不初始化数据库。

mod actions;
mod development;
mod errors;
mod handlers;
mod middleware;
mod projection;
mod request;
mod runtime;
mod session;
mod static_files;
mod uploads;
mod wiring;

pub use development::{DevelopmentPhase, DevelopmentProxy};
pub use wiring::SetupError;

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use tokio::sync::{RwLock, watch};

use errors::ApiError;
use middleware::{check_generation, cookie, header_once, identity, require_write, role};
use wiring::{HttpAuth, Published, Resources};

const BODY_LIMIT: usize = 1024 * 1024;
const COOKIE_NAME: &str = "kenvoy_session";

struct Shared {
    model: Published,
    resources: Resources,
    auth: HttpAuth,
    origin: String,
    authority: String,
    secure_cookie: bool,
    request_limits: Mutex<BTreeMap<String, (Instant, u32)>>,
    development: RwLock<Option<development::Development>>,
    development_changed: watch::Sender<u64>,
}

#[derive(Clone, Copy)]
struct ClientPeer(Option<SocketAddr>);

#[derive(Clone)]
pub struct HttpRuntime {
    shared: Arc<Shared>,
}

#[cfg(test)]
mod tests;
