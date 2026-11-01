use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_requests_use_one_connection_and_queue_rejection_is_known_not_executed()
-> TestResult {
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let owner = database.clone();
    let blocking = tokio::spawn(async move {
        owner
            .submit(move |_| {
                let _ = started.send(());
                blocked
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| DatabaseError::InvalidState("测试未释放阻塞任务".into()))?;
                Ok(())
            })
            .await
    });
    ready.await?;
    let count = Arc::new(AtomicUsize::new(0));
    let mut responses = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
        let count = count.clone();
        let (reply, response) = oneshot::channel();
        database
            .shared
            .sender
            .try_send(Command::Run(Box::new(move |_, cancelled| {
                if !cancelled {
                    count.fetch_add(1, Ordering::SeqCst);
                }
                let _ = reply.send(cancelled);
            })))
            .map_err(|_| "队列容量不正确")?;
        responses.push(response);
    }
    assert!(matches!(
        database
            .create("customers", json!({"name":"队列拒绝"}), None)
            .await,
        Err(DatabaseError::Busy)
    ));
    release.send(())?;
    blocking.await??;
    for response in responses {
        assert!(!response.await?);
    }
    assert_eq!(count.load(Ordering::SeqCst), QUEUE_CAPACITY);
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        0
    );
    let mut writes = Vec::new();
    for index in 0..20 {
        let owner = database.clone();
        writes.push(tokio::spawn(async move {
            owner
                .create(
                    "customers",
                    json!({"name":format!("并发{index}"),"age":index}),
                    None,
                )
                .await
        }));
    }
    for write in writes {
        write.await??;
    }
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        20
    );
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn dropping_an_accepted_write_response_does_not_claim_the_write_was_cancelled() -> TestResult
{
    let fixture = Fixture::new()?;
    let database = Database::open(fixture.database.clone(), fixture.application(BASIC)?).await?;
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let owner = database.clone();
    let blocking = tokio::spawn(async move {
        owner
            .submit(move |_| {
                let _ = started.send(());
                blocked
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| DatabaseError::InvalidState("测试未释放阻塞任务".into()))?;
                Ok(())
            })
            .await
    });
    ready.await?;
    let owner = database.clone();
    let accepted = tokio::spawn(async move {
        owner
            .create("customers", json!({"name":"响应丢弃仍需核对"}), None)
            .await
    });
    for _ in 0..100 {
        if database.shared.sender.capacity() < QUEUE_CAPACITY {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(database.shared.sender.capacity(), QUEUE_CAPACITY - 1);
    accepted.abort();
    let _ = accepted.await;
    release.send(())?;
    blocking.await??;
    assert_eq!(
        database
            .list("customers", ListQuery::default())
            .await?
            .total,
        1
    );
    database.shutdown().await?;
    Ok(())
}
