use super::*;

pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) app: ApplicationIR,
    pub(super) database: Database,
    pub(super) auth: Auth,
    pub(super) runtime: HttpRuntime,
}
impl Fixture {
    pub(super) async fn new(source: &str) -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "kenvoy-http-{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root)?;
        let entry = root.join("app.tsx");
        fs::write(&entry, source)?;
        let program = kengine::Engine::new().analyze_project(
            &root,
            kengine::AnalyzeOptions {
                entry_points: vec![entry.clone()],
                ..Default::default()
            },
        )?;
        let result = crate::application::compilation::analyze(
            &program,
            &entry.canonicalize()?,
            "测试",
            &crate::infrastructure::javascript::SwcClientCodeTransformer,
        );
        let app = result
            .application
            .ok_or_else(|| format!("{:?}", result.diagnostics))?;
        let database = Database::open(root.join("app.sqlite"), app.clone()).await?;
        let auth = Auth::new(database.clone(), SystemCredentials).await?;
        auth.init_admin("admin", PASSWORD).await?;
        let runtime = HttpRuntime::new(
            app.clone(),
            database.clone(),
            auth.clone(),
            "http://127.0.0.1:3000".into(),
            None,
            Default::default(),
        )
        .await?;
        Ok(Self {
            root,
            app,
            database,
            auth,
            runtime,
        })
    }
    pub(super) async fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: impl Into<Body>,
    ) -> TestResult<(StatusCode, HeaderMap, Bytes)> {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "127.0.0.1:3000");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = self
            .runtime
            .router()
            .oneshot(request.body(body.into())?)
            .await?;
        let (parts, body) = response.into_parts();
        Ok((
            parts.status,
            parts.headers,
            to_bytes(body, 2 * BODY_LIMIT).await?,
        ))
    }
    pub(super) async fn context(&self) -> TestResult<(String, String)> {
        let (status, headers, body) = self
            .request("GET", "/_kenvoy/session", &[], Body::empty())
            .await?;
        assert_eq!(status, StatusCode::OK);
        let cookie = headers
            .get(header::SET_COOKIE)
            .ok_or("缺少 Cookie")?
            .to_str()?
            .split(';')
            .next()
            .ok_or("Cookie 无效")?
            .to_owned();
        let body: Value = serde_json::from_slice(&body)?;
        Ok((
            cookie,
            body["csrf_token"].as_str().ok_or("缺少 CSRF")?.into(),
        ))
    }
    pub(super) async fn login(&self) -> TestResult<(String, String)> {
        self.login_as("admin").await
    }
    pub(super) async fn login_as(&self, username: &str) -> TestResult<(String, String)> {
        let (cookie, csrf) = self.context().await?;
        let (status, headers, body) = self
            .request(
                "POST",
                "/_kenvoy/login",
                &[
                    ("cookie", &cookie),
                    ("origin", "http://127.0.0.1:3000"),
                    ("x-kenvoy-csrf", &csrf),
                    ("content-type", "application/json"),
                ],
                json!({"username":username,"password":PASSWORD}).to_string(),
            )
            .await?;
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let next = headers
            .get(header::SET_COOKIE)
            .ok_or("缺少新 Cookie")?
            .to_str()?
            .split(';')
            .next()
            .ok_or("Cookie 无效")?
            .to_owned();
        assert_ne!(next, cookie);
        let body: Value = serde_json::from_slice(&body)?;
        Ok((next, body["csrf_token"].as_str().ok_or("缺少 CSRF")?.into()))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
