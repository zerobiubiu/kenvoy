use super::*;

#[tokio::test]
async fn shutdown_reaps_connection_is_idempotent_and_closes_every_clone() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(BASIC)?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let clone = database.clone();
    database
        .create("customers", json!({"name":"关闭后保留"}), None)
        .await?;
    tokio::time::timeout(Duration::from_secs(2), database.shutdown()).await??;
    assert!(
        database
            .shared
            .thread
            .lock()
            .map_err(|_| "线程句柄锁损坏")?
            .is_none()
    );
    clone.shutdown().await?;
    assert!(matches!(clone.info().await, Err(DatabaseError::Closed)));
    let reopened = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(
        reopened
            .list("customers", ListQuery::default())
            .await?
            .total,
        1
    );
    reopened.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn shutdown_interrupts_long_running_sql_and_reaps_the_owned_thread() -> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let owner = database.clone();
    let running = tokio::spawn(async move {
        owner.submit(move |state| {
        let _ = started.send(());
        state.connection.query_row("WITH RECURSIVE series(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM series) SELECT sum(n) FROM series", [], |row| row.get::<_, i64>(0)).map_err(DatabaseError::from)
    }).await
    });
    ready.await?;
    tokio::time::timeout(Duration::from_secs(14), database.shutdown()).await??;
    assert!(
        database
            .shared
            .thread
            .lock()
            .map_err(|_| "线程句柄锁损坏")?
            .is_none()
    );
    assert!(
        matches!(running.await?, Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))) if error.code == rusqlite::ErrorCode::OperationInterrupted)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_independent_databases_reopen_without_owning_old_connections() -> TestResult {
    let mut tasks = Vec::new();
    for _ in 0..8 {
        tasks.push(tokio::spawn(async move {
            for _ in 0..6 {
                let fixture = Fixture::new()?;
                let app = fixture.application(BASIC)?;
                let database = Database::open(fixture.database.clone(), app.clone())
                    .await
                    .map_err(|error| format!("首次打开 {}：{error}", fixture.database.display()))?;
                database
                    .create("customers", json!({"name":"并行生命周期"}), None)
                    .await
                    .map_err(|error| format!("首次写入 {}：{error}", fixture.database.display()))?;
                database
                    .shutdown()
                    .await
                    .map_err(|error| format!("关闭 {}：{error}", fixture.database.display()))?;
                let reopened = Database::open(fixture.database.clone(), app)
                    .await
                    .map_err(|error| format!("重新打开 {}：{error}", fixture.database.display()))?;
                assert_eq!(
                    reopened
                        .list("customers", ListQuery::default())
                        .await?
                        .total,
                    1
                );
                reopened
                    .create("customers", json!({"name":"重新写入"}), None)
                    .await
                    .map_err(|error| format!("重新写入 {}：{error}", fixture.database.display()))?;
                reopened.shutdown().await?;
            }
            Ok::<_, Box<dyn Error + Send + Sync>>(())
        }));
    }
    for task in tasks {
        task.await??;
    }
    Ok(())
}
