//! 国际化验收只使用真实 CLI、HTTP 和本测试拥有的 SQLite。

use std::{collections::BTreeMap, fs, path::PathBuf, process::Output, time::Duration};

use kenvoy_tests::{Fixture, TestResult, require_success};
use reqwest::{Client, Method, StatusCode, header};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

pub(super) const PASSWORD: &str = "isolated-i18n-password-5824";

pub(super) fn project(label: &str) -> TestResult<Fixture> {
    let fixture = Fixture::new(label)?;
    fixture.create()?;
    Ok(fixture)
}

pub(super) fn command(fixture: &Fixture, args: &[&str]) -> TestResult<Output> {
    let mut command = vec!["--project", "app"];
    command.extend_from_slice(args);
    fixture.command(&command, "")
}

pub(super) fn build(fixture: &Fixture) -> TestResult<Value> {
    require_success(&command(fixture, &["build"])?)?;
    manifest(fixture)
}

pub(super) fn manifest(fixture: &Fixture) -> TestResult<Value> {
    Ok(serde_json::from_slice(&fs::read(
        fixture.project().join("dist/manifest.json"),
    )?)?)
}

pub(super) fn version(fixture: &Fixture, manifest: &Value) -> TestResult<PathBuf> {
    Ok(fixture
        .project()
        .join("dist/versions")
        .join(manifest["build_id"].as_str().ok_or("清单缺少构建 ID")?))
}

pub(super) fn archive_sources(fixture: &Fixture) -> TestResult {
    for name in ["src", "locales", "migrations", ".kenvoy"] {
        let path = fixture.project().join(name);
        if path.exists() {
            fs::rename(path, fixture.root().join(format!("archive-{name}")))?;
        }
    }
    Ok(())
}

pub(super) fn no_node(fixture: &Fixture) -> TestResult<BTreeMap<String, String>> {
    let path = fixture.root().to_str().ok_or("夹具路径不是 UTF-8")?;
    Ok(BTreeMap::from([
        ("PATH".into(), path.into()),
        ("Path".into(), path.into()),
    ]))
}

pub(super) fn read_db(fixture: &Fixture) -> TestResult<Connection> {
    Ok(Connection::open_with_flags(
        fixture.project().join("data/app.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

pub(super) fn schema(fixture: &Fixture) -> TestResult<Vec<(String, String)>> {
    let database = read_db(fixture)?;
    let mut statement = database
        .prepare("SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")?;
    Ok(statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(super) fn applied(fixture: &Fixture) -> TestResult<Value> {
    let output = command(fixture, &["migrate", "history"])?;
    require_success(&output)?;
    Ok(serde_json::from_slice::<Value>(&output.stdout)?["applied"].clone())
}

pub(super) fn stored(fixture: &Fixture, id: i64) -> TestResult<(String, String, i64)> {
    Ok(read_db(fixture)?.query_row(
        "SELECT name,phone,age FROM customers WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

pub(super) fn account(fixture: &Fixture, operation: &str, username: &str) -> TestResult {
    require_success(&fixture.command(
        &[
            "--project",
            "app",
            "admin",
            operation,
            "--username",
            username,
            "--password-stdin",
        ],
        &format!("{PASSWORD}\n"),
    )?)
}

pub(super) struct Session {
    pub(super) client: Client,
    pub(super) origin: String,
    cookie: Option<String>,
    csrf: Option<String>,
    generation: Option<u64>,
}
impl Session {
    pub(super) async fn new(origin: &str) -> TestResult<Self> {
        let mut session = Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            origin: origin.into(),
            cookie: None,
            csrf: None,
            generation: None,
        };
        session.get("/_kenvoy/session", StatusCode::OK).await?;
        Ok(session)
    }
    pub(super) async fn get(&mut self, path: &str, status: StatusCode) -> TestResult<Value> {
        self.request(Method::GET, path, None, None, status).await
    }
    pub(super) async fn post(
        &mut self,
        path: &str,
        data: Value,
        form: Option<&str>,
        status: StatusCode,
    ) -> TestResult<Value> {
        self.request(Method::POST, path, Some(data), form, status)
            .await
    }
    async fn request(
        &mut self,
        method: Method,
        path: &str,
        data: Option<Value>,
        form: Option<&str>,
        status: StatusCode,
    ) -> TestResult<Value> {
        let mut request = self
            .client
            .request(method.clone(), format!("{}{path}", self.origin))
            .header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9");
        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if method == Method::POST {
            request = request
                .header(header::ORIGIN, &self.origin)
                .header("x-kenvoy-csrf", self.csrf.as_ref().ok_or("会话缺少 CSRF")?)
                .header(
                    "x-kenvoy-generation",
                    self.generation.ok_or("会话缺少代际")?.to_string(),
                );
        }
        if let Some(form) = form {
            request = request.header("x-kenvoy-form", form);
        }
        if let Some(data) = data {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&data)?);
        }
        let response = request.send().await?;
        let actual = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()?;
        let request_id = response
            .headers()
            .get("x-request-id")
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()?;
        let bytes = response.bytes().await?;
        assert_eq!(actual, status, "{method} {path}");
        if let Some(cookie) = cookie {
            assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
            self.cookie = Some(cookie.split(';').next().ok_or("Cookie 格式错误")?.into());
        }
        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        if let Some(csrf) = value["csrf_token"].as_str() {
            self.csrf = Some(csrf.into());
            self.generation = Some(value["generation"].as_u64().ok_or("响应缺少代际")?);
        }
        if let Some(error) = value.get("error").and_then(Value::as_object) {
            assert_eq!(error["request_id"].as_str(), request_id.as_deref());
        }
        Ok(value)
    }
    pub(super) async fn login(&mut self, username: &str) -> TestResult {
        self.post(
            "/_kenvoy/login",
            json!({"username":username,"password":PASSWORD}),
            None,
            StatusCode::OK,
        )
        .await?;
        Ok(())
    }
    pub(super) async fn logout(&mut self) -> TestResult {
        self.post("/_kenvoy/logout", json!({}), None, StatusCode::OK)
            .await?;
        Ok(())
    }
}

pub(super) fn form(model: &Value) -> TestResult<String> {
    Ok(model["pages"]
        .as_array()
        .ok_or("描述缺少页面")?
        .iter()
        .filter_map(|page| page["components"].as_array())
        .flatten()
        .find(|node| node["type"] == "form")
        .and_then(|node| node["id"].as_str())
        .ok_or("描述缺少公开表单")?
        .into())
}
