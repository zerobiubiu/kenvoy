//! `MinIO` / S3 集成测试。
//!
//! 需设置环境变量 `KENVOY_TEST_S3_*`（参见 `test_cfg`），未设置时测试自动跳过。
//! 端点默认 `http://127.0.0.1:9000`，可覆盖；默认走 `kenvoy-test-<pid>` bucket。

use kengine_s3::{ObjectStore, S3Config};

fn test_cfg() -> Option<S3Config> {
    let endpoint = std::env::var("KENVOY_TEST_S3_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string());
    let access_key =
        std::env::var("KENVOY_TEST_S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret_key =
        std::env::var("KENVOY_TEST_S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    if std::env::var("KENVOY_TEST_S3_DISABLE").is_ok() {
        return None;
    }
    let bucket = format!("kenvoy-test-{}", std::process::id());
    Some(S3Config {
        endpoint,
        region: "us-east-1".to_string(),
        bucket,
        access_key,
        secret_key,
    })
}

async fn setup_store() -> Option<ObjectStore> {
    let cfg = test_cfg()?;
    let store = ObjectStore::connect(&cfg).expect("连接配置失败");
    store.ensure_bucket().await.expect("ensure bucket 失败");
    Some(store)
}

#[tokio::test]
async fn put_and_get_round_trip() {
    let Some(store) = setup_store().await else {
        eprintln!("跳过：未配置 MinIO 集成测试环境");
        return;
    };
    let key = ObjectStore::object_key("hello.txt");
    let content = b"hello kenvoy s3".to_vec();
    store
        .put(&key, &content, "text/plain")
        .await
        .expect("上传失败");
    let got = store.get(&key).await.expect("下载失败");
    assert_eq!(got, content);
    store.delete(&key).await.expect("删除失败");
}

#[tokio::test]
async fn object_key_sanitizes_filename() {
    let key = ObjectStore::object_key("我的文件.pdf");
    assert!(key.starts_with("uploads/"));
    assert!(key.ends_with("____.pdf"));
}

#[tokio::test]
async fn get_missing_object_errors() {
    let Some(store) = setup_store().await else {
        eprintln!("跳过：未配置 MinIO 集成测试环境");
        return;
    };
    let key = ObjectStore::object_key("definitely-not-exists.bin");
    let res = store.get(&key).await;
    assert!(res.is_err());
}
