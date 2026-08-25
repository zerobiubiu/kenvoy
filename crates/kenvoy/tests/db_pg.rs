//! PostgreSQL 集成测试（基准 PG 18+）。
//!
//! 需设置环境变量 `KENVOY_TEST_DATABASE_URL`（如 `postgres://kenvoy:kenvoy@127.0.0.1:5433/kenvoy`），
//! 未设置时测试自动跳过。

#![cfg(feature = "db")]

use std::collections::HashMap;

use kengine::ir::{FieldDef, FieldType, Model};
use kenvoy::db::{ModelStore, StorageStore, UserStore, connect, migrate};
use serde_json::json;
use sqlx::PgPool;

fn db_url() -> Option<String> {
    std::env::var("KENVOY_TEST_DATABASE_URL").ok()
}

/// 生成测试用唯一后缀（避免并发/重复执行冲突）。
fn uniq() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时间异常")
        .as_nanos()
}

/// 迁移串行锁：多个测试并行执行 `migrate` 时，并发 `CREATE TABLE IF NOT EXISTS`
/// 可能触发 `pg_class` 唯一键竞态（新表首次创建时）。
static MIGRATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn setup_pool() -> Option<PgPool> {
    let url = db_url()?;
    let pool = connect(&url).await.expect("连接失败");
    let guard = MIGRATE_LOCK.lock().await;
    migrate(&pool).await.expect("迁移失败");
    drop(guard);
    Some(pool)
}

fn leave_model() -> Model {
    Model {
        name: "请假单".to_string(),
        fields: vec![
            FieldDef {
                name: "请假类型".to_string(),
                field_type: FieldType::Select,
                label: None,
                options: vec![],
                required: false,
            },
            FieldDef {
                name: "请假天数".to_string(),
                field_type: FieldType::Number,
                label: None,
                options: vec![],
                required: false,
            },
            FieldDef {
                name: "事由".to_string(),
                field_type: FieldType::Textarea,
                label: None,
                options: vec![],
                required: false,
            },
        ],
    }
}

#[tokio::test]
async fn model_crud_with_chinese_columns() {
    let Some(pool) = setup_pool().await else {
        eprintln!("跳过：未设置 KENVOY_TEST_DATABASE_URL");
        return;
    };
    let store = ModelStore::new(&pool);
    let model = leave_model();
    store.ensure(&model).await.expect("建表失败");

    let mut data = HashMap::new();
    data.insert("请假类型".to_string(), json!("年假"));
    data.insert("请假天数".to_string(), json!(2));
    data.insert("事由".to_string(), json!("家中有事"));
    let id = store.insert(&model, &data).await.expect("插入失败");
    assert!(id > 0);

    let rows = store.list(&model).await.expect("查询失败");
    let mine = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(id))
        .expect("新记录应存在");
    assert_eq!(mine["请假类型"], "年假");
    assert_eq!(mine["事由"], "家中有事");
    assert_eq!(mine["请假天数"].as_f64(), Some(2.0));
}

#[tokio::test]
async fn model_insert_with_missing_fields_uses_defaults() {
    let Some(pool) = setup_pool().await else {
        eprintln!("跳过：未设置 KENVOY_TEST_DATABASE_URL");
        return;
    };
    let store = ModelStore::new(&pool);
    let model = leave_model();
    store.ensure(&model).await.expect("建表失败");

    let empty = HashMap::new();
    let id = store.insert(&model, &empty).await.expect("插入失败");
    assert!(id > 0);
    let rows = store.list(&model).await.expect("查询失败");
    assert!(rows.iter().any(|r| r["id"].as_i64() == Some(id)));
}

#[tokio::test]
async fn user_auth_session_flow() {
    let Some(pool) = setup_pool().await else {
        eprintln!("跳过：未设置 KENVOY_TEST_DATABASE_URL");
        return;
    };
    let store = UserStore::new(&pool);
    let username = format!("zhangsan{}", uniq());
    let uid = store
        .create_user(&username, "secret123", "张三")
        .await
        .expect("创建用户失败");
    let user = store
        .verify_login(&username, "secret123")
        .await
        .expect("校验失败")
        .expect("登录应成功");
    assert_eq!(user.display_name, "张三");
    assert!(
        store
            .verify_login(&username, "wrong")
            .await
            .expect("校验失败")
            .is_none()
    );

    let token = store.create_session(uid).await.expect("创建会话失败");
    let by_session = store
        .user_by_session(&token)
        .await
        .expect("查询会话失败")
        .expect("会话用户应存在");
    assert_eq!(by_session.id, uid);
}

#[tokio::test]
async fn roles_levels_and_contacts() {
    let Some(pool) = setup_pool().await else {
        eprintln!("跳过：未设置 KENVOY_TEST_DATABASE_URL");
        return;
    };
    let store = UserStore::new(&pool);

    let hr_role = store
        .create_role(&format!("HR{}", uniq()), 50, Some("人事"))
        .await
        .expect("创建角色失败");
    let dept = store
        .create_department("人事部", None)
        .await
        .expect("创建部门失败");

    let uid = store
        .create_user(&format!("hr1{}", uniq()), "pw", "人事一号")
        .await
        .expect("创建用户失败");
    store.assign_role(uid, hr_role).await.expect("分配角色失败");
    assert_eq!(store.user_level(uid).await.expect("查询等级失败"), 50);

    // 有账号人员
    let c1 = store
        .add_contact("张三", Some(dept), Some(uid), None, None, Some("主管"))
        .await
        .expect("新增通讯录失败");
    // 无账号人员（人事办理新增但不用账户）
    let c2 = store
        .add_contact("临时工", Some(dept), None, None, None, None)
        .await
        .expect("新增通讯录失败");
    assert!(c1 > 0 && c2 > 0);

    let contacts = store.list_contacts(None).await.expect("查询失败");
    assert!(
        contacts
            .iter()
            .any(|c| c.name == "临时工" && c.user_id.is_none())
    );
    assert!(
        contacts
            .iter()
            .any(|c| c.name == "张三" && c.user_id == Some(uid))
    );

    let filtered = store.list_contacts(Some("张")).await.expect("查询失败");
    assert!(filtered.iter().all(|c| c.name.contains('张')));
    assert!(filtered.iter().any(|c| c.name == "张三"));
}

#[tokio::test]
async fn storage_object_metadata() {
    let Some(pool) = setup_pool().await else {
        eprintln!("跳过：未设置 KENVOY_TEST_DATABASE_URL");
        return;
    };
    let store = StorageStore::new(&pool);
    let id = store
        .create(
            "kenvoy-bucket",
            "uploads/a.pdf",
            "a.pdf",
            Some("application/pdf"),
            1024,
            Some("etag1"),
        )
        .await
        .expect("创建对象失败");
    let obj = store.get(&id).await.expect("查询失败").expect("对象应存在");
    assert_eq!(obj.bucket, "kenvoy-bucket");
    assert_eq!(obj.file_name, "a.pdf");
    assert_eq!(obj.size_bytes, 1024);
}
