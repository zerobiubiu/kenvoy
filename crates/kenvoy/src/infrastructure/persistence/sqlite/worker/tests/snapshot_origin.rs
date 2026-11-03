//! 复制单个 SQLite 文件也必须保留开发快照来源，不能借旧库基线转正。
use super::*;

#[tokio::test]
async fn regression_snapshot_origin_survives_single_file_copy() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(BASIC)?;
    let snapshot = fixture.root.join("dev-snapshots/preview.sqlite");
    let database = Database::open_snapshot(snapshot.clone(), app.clone()).await?;
    database
        .create("customers", json!({"name":"快照不得转正"}), None)
        .await?;
    database.shutdown().await?;
    fs::create_dir_all(fixture.database.parent().ok_or("缺少正式数据目录")?)?;
    fs::copy(&snapshot, &fixture.database)?;
    migration::catalog::generate(&fixture.root, &app, "正式初始结构", None)?;
    migration::catalog::seal(&fixture.root, &app, None)?;
    let catalog = migration::catalog::read(&fixture.root)?;
    let rejected = crate::infrastructure::persistence::sqlite::snapshot::reject(&fixture.database);
    let adopted = migration::catalog::baseline(&fixture.database, &catalog, "000001", None, None);
    assert!(
        rejected.is_err() && adopted.is_err(),
        "复制快照不得绕过来源检查或进入 baseline：{rejected:?} / {adopted:?}"
    );
    let connection = Connection::open(&fixture.database)?;
    assert_eq!(
        connection.query_row(
            "SELECT count(*) FROM customers WHERE name='快照不得转正'",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    Ok(())
}
