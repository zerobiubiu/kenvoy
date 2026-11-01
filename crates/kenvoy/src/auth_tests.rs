use super::*;
use crate::application::ApplicationIR;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
const PASSWORD: &str = "认证测试Password2026!";
struct Fixture {
    root: PathBuf,
    path: PathBuf,
    app: ApplicationIR,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "kenvoy-auth-{}-{}-{sequence}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir(&root)?;
        let entry = root.join("app.tsx");
        fs::write(
            &entry,
            "export default <app name='认证验证'><resource name='customers'><text name='name' required/></resource></app>;",
        )?;
        let program = kengine::Engine::new().analyze_project(
            &root,
            kengine::AnalyzeOptions {
                entry_points: vec![entry.clone()],
                ..Default::default()
            },
        )?;
        let app = crate::dsl::analyze(&program, &entry.canonicalize()?, "认证验证")
            .application
            .ok_or("测试 DSL 分析失败")?;
        Ok(Self {
            path: root.join("app.sqlite"),
            root,
            app,
        })
    }
    async fn open(&self) -> TestResult<(Database, Auth)> {
        let database = Database::open(self.path.clone(), self.app.clone()).await?;
        let auth = Auth::new(database.clone()).await?;
        Ok((database, auth))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[tokio::test]
async fn passwords_roles_rotation_csrf_and_restart_use_real_persistent_storage() -> TestResult {
    let fixture = Fixture::new()?;
    let (database, auth) = fixture.open().await?;
    let admin = auth.init_admin("Admin", PASSWORD).await?;
    assert_eq!(admin.username, "admin");
    let ordinary = auth.add_user("Member", PASSWORD, Role::User).await?;
    assert_eq!(ordinary.role, Role::User);
    let anonymous = auth.session(None).await?;
    assert!(anonymous.view.user.is_none());
    let raw = anonymous.set_token.ok_or("没有新 Session")?;
    assert!(Auth::verify_csrf(&raw, &anonymous.view.csrf_token));
    assert!(!Auth::verify_csrf(&raw, &"0".repeat(64)));
    assert!(!Auth::verify_csrf(
        &format!("A{}", &raw[1..]),
        &anonymous.view.csrf_token
    ));
    assert!(auth.session(Some(&raw)).await?.set_token.is_none());
    for (username, password) in [
        ("missing", PASSWORD),
        ("admin", "incorrect-password"),
        ("bad username", PASSWORD),
    ] {
        assert!(matches!(
            auth.login(&raw, username, password).await,
            Err(AuthError::InvalidCredentials)
        ));
    }
    let login = auth.login(&raw, "ADMIN", PASSWORD).await?;
    assert_eq!(login.view.user, Some(admin));
    let logged = login.set_token.ok_or("登录没有旋转 Session")?;
    assert_ne!(raw, logged);
    assert!(auth.authenticate(&raw).await?.is_none());
    assert!(!Auth::verify_csrf(&logged, &anonymous.view.csrf_token));
    let json = serde_json::to_string(&login.view)?;
    assert!(!json.contains(PASSWORD));
    assert!(!json.contains(&logged));
    assert!(!json.contains("password_hash"));
    let inspect = rusqlite::Connection::open(&fixture.path)?;
    let hashes: Vec<String> = inspect
        .prepare("SELECT password_hash FROM _kenvoy_users ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(hashes.len(), 2);
    assert_ne!(hashes[0], hashes[1]);
    for hash in &hashes {
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password(PASSWORD.as_bytes(), hash)?);
    }
    let stored: String =
        inspect.query_row("SELECT token_hash FROM _kenvoy_sessions", [], |row| {
            row.get(0)
        })?;
    assert_ne!(stored, logged);
    assert_eq!(
        stored,
        hex(&Sha256::digest(
            decode_token(&logged).ok_or("Session 编码无效")?
        ))
    );
    drop(inspect);
    drop(auth);
    database.shutdown().await?;
    let (reopened, auth) = fixture.open().await?;
    assert_eq!(
        auth.authenticate(&logged)
            .await?
            .ok_or("持久化 Session 丢失")?
            .user
            .as_ref()
            .map(|user| user.role),
        Some(Role::Admin)
    );
    let logout = auth.logout(&logged).await?;
    assert!(logout.view.user.is_none());
    assert!(auth.authenticate(&logged).await?.is_none());
    let raw = logout.set_token.ok_or("登出没有新匿名 Session")?;
    let member = auth.login(&raw, "member", PASSWORD).await?;
    assert_eq!(member.view.user, Some(ordinary));
    drop(auth);
    reopened.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn credentials_and_password_work_are_bounded_without_admin_overwrite() -> TestResult {
    let fixture = Fixture::new()?;
    let (database, auth) = fixture.open().await?;
    for username in ["ab", "../admin", "中文名", " user", "x".repeat(65).as_str()] {
        assert!(matches!(
            auth.init_admin(username, PASSWORD).await,
            Err(AuthError::InvalidInput(_))
        ));
    }
    for password in [
        "short",
        "abcdefghijkl\0",
        "abcdefghijkl\r",
        "            ",
        &"密".repeat(43),
    ] {
        assert!(matches!(
            auth.init_admin("admin", password).await,
            Err(AuthError::InvalidInput(_))
        ));
    }
    auth.init_admin("admin", PASSWORD).await?;
    assert!(matches!(
        auth.init_admin("other", PASSWORD).await,
        Err(AuthError::AlreadyInitialized)
    ));
    assert!(matches!(
        auth.add_user("ADMIN", PASSWORD, Role::User).await,
        Err(AuthError::UsernameExists)
    ));
    assert!(matches!(
        auth.add_user("anonymous", PASSWORD, Role::Anonymous).await,
        Err(AuthError::InvalidInput(_))
    ));
    let first = auth.password_slots.clone().try_acquire_owned()?;
    let second = auth.password_slots.clone().try_acquire_owned()?;
    assert!(matches!(
        auth.add_user("another", PASSWORD, Role::User).await,
        Err(AuthError::Overloaded)
    ));
    drop(first);
    drop(second);
    assert!(matches!(
        auth.login(&"0".repeat(64), "admin", PASSWORD).await,
        Err(AuthError::InvalidSession)
    ));
    assert!(matches!(
        verify_password(b"password", "$argon2id$v=19$m=999999999,t=2,p=1$bad$bad"),
        Err(AuthError::Crypto)
    ));
    drop(auth);
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn same_session_cannot_be_consumed_by_two_concurrent_logins() -> TestResult {
    let fixture = Fixture::new()?;
    let (database, auth) = fixture.open().await?;
    auth.init_admin("admin", PASSWORD).await?;
    let raw = auth
        .session(None)
        .await?
        .set_token
        .ok_or("匿名 Session 缺失")?;
    let (first, second) = tokio::join!(
        auth.login(&raw, "admin", PASSWORD),
        auth.login(&raw, "admin", PASSWORD)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let failure = if first.is_err() { first } else { second };
    assert!(matches!(failure, Err(AuthError::InvalidSession)));
    drop(auth);
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn expired_sessions_cannot_login_and_bootstrap_issues_a_fresh_context() -> TestResult {
    let fixture = Fixture::new()?;
    let (database, auth) = fixture.open().await?;
    let original = auth.session(None).await?;
    let raw = original.set_token.ok_or("没有初始 Session")?;
    let external = rusqlite::Connection::open(&fixture.path)?;
    external.execute("UPDATE _kenvoy_sessions SET created_at=1,expires_at=2", [])?;
    drop(external);
    assert!(auth.authenticate(&raw).await?.is_none());
    assert!(matches!(
        auth.login(&raw, "admin", PASSWORD).await,
        Err(AuthError::InvalidSession)
    ));
    assert!(matches!(
        auth.logout(&raw).await,
        Err(AuthError::InvalidSession)
    ));
    let replacement = auth.session(Some(&raw)).await?;
    assert_ne!(replacement.set_token.as_deref(), Some(raw.as_str()));
    assert!(replacement.view.user.is_none());
    assert!(replacement.view.expires_at > now()?);
    assert!(!Auth::verify_csrf(
        replacement.set_token.as_deref().ok_or("没有替代 Session")?,
        &original.view.csrf_token
    ));
    drop(auth);
    database.shutdown().await?;
    Ok(())
}
