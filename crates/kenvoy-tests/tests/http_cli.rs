//! 真实 CLI、HTTP、角色权限与 SQLite 重启持久化；不使用替代服务或内存数据。

use std::time::Duration;

use kenvoy_tests::{Fixture, TestResult, require_success};
use reqwest::{Client, Method, StatusCode, header};
use serde_json::{Value, json};

struct Session {
    client: Client,
    origin: String,
    cookie: Option<String>,
    csrf: Option<String>,
    generation: Option<u64>,
    assertions: usize,
}
impl Session {
    fn new(origin: &str) -> TestResult<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            origin: origin.into(),
            cookie: None,
            csrf: None,
            generation: None,
            assertions: 0,
        })
    }
    async fn request(
        &mut self,
        method: Method,
        path: &str,
        data: Option<Value>,
        form: Option<&str>,
        expected: StatusCode,
    ) -> TestResult<Value> {
        let mut request = self
            .client
            .request(method.clone(), format!("{}{path}", self.origin));
        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        if !matches!(method, Method::GET | Method::HEAD) {
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
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()?;
        let bytes = response.bytes().await?;
        // 响应内容可能涉及认证；断言只展示方法、路径与状态，不打印密码或 Cookie。
        assert_eq!(status, expected, "{method} {path}");
        self.assertions += 1;
        if let Some(cookie) = set_cookie {
            assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
            self.cookie = Some(cookie.split(';').next().ok_or("Cookie 格式无效")?.into());
        }
        let value: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        if let Some(csrf) = value["csrf_token"].as_str() {
            self.csrf = Some(csrf.into());
            self.generation = Some(value["generation"].as_u64().ok_or("缺少代际")?);
        }
        Ok(value)
    }
}

#[tokio::test]
async fn real_cli_http_roles_and_restart_persist_sqlite() -> TestResult {
    let fixture = Fixture::new("http-cli")?;
    fixture.create()?;
    let password = "isolated-http-test-password-472";
    for args in [
        [
            "--project",
            "app",
            "admin",
            "init",
            "--username",
            "administrator",
            "--password-stdin",
        ],
        [
            "--project",
            "app",
            "admin",
            "add-user",
            "--username",
            "reader",
            "--password-stdin",
        ],
    ] {
        require_success(&fixture.command(&args, &format!("{password}\n"))?)?;
    }
    let mut server = fixture.start("dev")?;
    let mut session = Session::new(server.origin())?;
    session
        .request(Method::GET, "/_kenvoy/session", None, None, StatusCode::OK)
        .await?;
    let model = session
        .request(
            Method::GET,
            "/_kenvoy/application",
            None,
            None,
            StatusCode::OK,
        )
        .await?;
    assert_eq!(model["format_version"], 3);
    assert!(
        !model.to_string().contains(
            fixture
                .root()
                .file_name()
                .ok_or("夹具目录名缺失")?
                .to_string_lossy()
                .as_ref()
        )
    );
    let form = model["pages"][0]["components"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| node["type"] == "form"))
        .and_then(|node| node["id"].as_str())
        .ok_or("应用缺少登记表单")?;
    session
        .request(
            Method::POST,
            "/api/customers",
            Some(json!({"name":"","age":-1})),
            Some(form),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await?;
    let created = session
        .request(
            Method::POST,
            "/api/customers",
            Some(json!({"name":"张三","phone":"13800000000","age":25})),
            Some(form),
            StatusCode::CREATED,
        )
        .await?;
    let id = created["data"]["id"].as_i64().ok_or("新增记录缺少 id")?;
    assert_eq!(created["data"]["name"], "张三");
    let record_path = format!("/api/customers/{id}");
    session
        .request(
            Method::GET,
            "/api/customers",
            None,
            None,
            StatusCode::UNAUTHORIZED,
        )
        .await?;
    session
        .request(
            Method::GET,
            &record_path,
            None,
            None,
            StatusCode::UNAUTHORIZED,
        )
        .await?;
    session
        .request(
            Method::PATCH,
            &record_path,
            Some(json!({"age":26})),
            None,
            StatusCode::UNAUTHORIZED,
        )
        .await?;
    session
        .request(
            Method::POST,
            "/_kenvoy/login",
            Some(json!({"username":"reader","password":password})),
            None,
            StatusCode::OK,
        )
        .await?;
    session
        .request(
            Method::GET,
            "/api/customers",
            None,
            None,
            StatusCode::FORBIDDEN,
        )
        .await?;
    session
        .request(
            Method::POST,
            "/_kenvoy/logout",
            Some(json!({})),
            None,
            StatusCode::OK,
        )
        .await?;
    session
        .request(
            Method::POST,
            "/_kenvoy/login",
            Some(json!({"username":"administrator","password":password})),
            None,
            StatusCode::OK,
        )
        .await?;
    let list = session
        .request(
            Method::GET,
            "/api/customers?search=%E5%BC%A0&page=1&page_size=20",
            None,
            None,
            StatusCode::OK,
        )
        .await?;
    assert_eq!(list["total"], 1);
    assert_eq!(list["data"][0]["id"], id);
    session
        .request(
            Method::PATCH,
            &record_path,
            Some(json!({"age":26})),
            None,
            StatusCode::OK,
        )
        .await?;
    session
        .request(
            Method::GET,
            "/api/customers?page=1&page=2",
            None,
            None,
            StatusCode::BAD_REQUEST,
        )
        .await?;
    session
        .request(
            Method::GET,
            "/api/_kenvoy_users",
            None,
            None,
            StatusCode::NOT_FOUND,
        )
        .await?;
    server.stop()?;
    {
        let database = rusqlite::Connection::open_with_flags(
            fixture.project().join("data/app.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let stored: (String, i64) =
            database.query_row("SELECT name,age FROM customers WHERE id=?1", [id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        assert_eq!(stored, ("张三".into(), 26));
    }
    server = fixture.start("dev")?;
    session.origin = server.origin().into();
    let restored = session
        .request(Method::GET, &record_path, None, None, StatusCode::OK)
        .await?;
    assert_eq!(restored["data"]["age"], 26);
    session
        .request(
            Method::DELETE,
            &record_path,
            None,
            None,
            StatusCode::NO_CONTENT,
        )
        .await?;
    session
        .request(Method::GET, &record_path, None, None, StatusCode::NOT_FOUND)
        .await?;
    session
        .request(
            Method::POST,
            "/_kenvoy/logout",
            Some(json!({})),
            None,
            StatusCode::OK,
        )
        .await?;
    session
        .request(
            Method::GET,
            "/api/customers",
            None,
            None,
            StatusCode::UNAUTHORIZED,
        )
        .await?;
    assert_eq!(session.assertions, 20, "原有真实 HTTP 验收应完整迁移");
    assert!(!server.logs()?.contains(password));
    server.stop()?;
    Ok(())
}
