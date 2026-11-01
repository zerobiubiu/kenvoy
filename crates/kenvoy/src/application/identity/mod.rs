//! 服务端身份与 Session；持久化端口只接收摘要，不保存 Cookie 原文。
mod error;
pub mod ports;
mod service;
mod views;

pub use error::AuthError;
pub use service::Auth;
pub use views::{SessionResponse, SessionView, UserView};
