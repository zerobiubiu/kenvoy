mod actions;
mod development;
mod fixture;
mod limits;
mod options;
mod publication;
mod resources;
mod routing;
mod security;
mod uploads;
mod websocket;

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};
use tower::ServiceExt;

use super::{
    BODY_LIMIT, DevelopmentPhase, DevelopmentProxy, HttpRuntime, errors::REQUEST_SEQUENCE,
    wiring::HttpAuth as Auth,
};
use crate::{
    domain::model::{ApplicationIR, Role},
    infrastructure::{identity::SystemCredentials, persistence::sqlite::Database},
};
use fixture::Fixture;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
const SOURCE: &str = r#"export default <app name="客户应用"><resource name="customers"><text name="name" required maxLength={100}/><text name="phone"/><number name="age" integer min={0}/></resource><page path="/customers"><form resource="customers"/></page><page path="/private" permission="admin"><table resource="customers"/></page></app>;"#;
const PASSWORD: &str = "测试密码-123456789";
const UPLOAD_SOURCE: &str = r#"export default <app name="附件测试"><resource name="documents"><text name="title" required unique/><file name="attachments" maxFiles={2} maxSize={4194304} accept={["application/pdf","image/*"]}/><file name="private_files"/></resource><page path="/submit"><form resource="documents" fields={["title","attachments"]}/></page></app>;"#;
