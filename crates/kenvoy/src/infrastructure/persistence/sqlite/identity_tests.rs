use super::*;
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn database() -> Result<Connection> {
    let mut db = Connection::open_in_memory()?;
    db.pragma_update(None, "foreign_keys", true)?;
    initialize(&mut db)?;
    Ok(db)
}

#[test]
fn auth_schema_rejects_partial_tables_versions_and_extra_owned_objects() -> TestResult {
    for attack in [
        "DROP TABLE _kenvoy_auth_schema",
        "PRAGMA ignore_check_constraints=ON; UPDATE _kenvoy_auth_schema SET version=2",
        "CREATE TRIGGER evil AFTER INSERT ON _kenvoy_users BEGIN DELETE FROM _kenvoy_sessions; END",
        "CREATE INDEX unexpected ON _kenvoy_users(created_at)",
        "DELETE FROM _kenvoy_auth_schema",
    ] {
        let mut db = database()?;
        db.execute_batch(attack)?;
        assert!(
            matches!(
                initialize(&mut db),
                Err(DatabaseError::Auth(StoreError::Schema(_)))
            ),
            "{attack}"
        );
    }
    Ok(())
}

#[test]
fn dangling_authenticated_session_never_becomes_anonymous() -> TestResult {
    let mut db = database()?;
    db.pragma_update(None, "foreign_keys", false)?;
    let hash = "a".repeat(64);
    db.execute(
        "INSERT INTO _kenvoy_sessions VALUES(?1,123,200,100)",
        [&hash],
    )?;
    assert!(matches!(
        lookup(&db, &hash, 150),
        Err(DatabaseError::Auth(StoreError::Schema(_)))
    ));
    assert!(matches!(
        initialize(&mut db),
        Err(DatabaseError::Auth(StoreError::Schema(_)))
    ));
    Ok(())
}

#[test]
fn capacity_failure_rolls_back_rotation_and_expired_cleanup_is_bounded() -> TestResult {
    let mut db = database()?;
    let user = add_user(&mut db, "admin", "phc", Role::Admin, true, 1)?;
    let old = "a".repeat(64);
    rotate(&mut db, None, &old, None, 1, 200)?;
    for index in 0..32 {
        rotate(
            &mut db,
            None,
            &format!("{index:064x}"),
            Some(user.id),
            1,
            200,
        )?;
    }
    assert!(matches!(
        rotate(
            &mut db,
            Some(&old),
            &"b".repeat(64),
            Some(user.id),
            100,
            200
        ),
        Err(DatabaseError::Auth(StoreError::SessionCapacity))
    ));
    assert!(lookup(&db, &old, 100)?.is_some());
    assert!(lookup(&db, &"b".repeat(64), 100)?.is_none());
    db.execute("DELETE FROM _kenvoy_sessions", [])?;
    let transaction = db.transaction()?;
    for index in 0..SESSION_CAPACITY {
        transaction.execute(
            "INSERT INTO _kenvoy_sessions(token_hash,expires_at,created_at) VALUES(?1,10,1)",
            [format!("{index:064x}")],
        )?;
    }
    transaction.commit()?;
    rotate(&mut db, None, &old, None, 20, 30)?;
    let count: i64 = db.query_row("SELECT count(*) FROM _kenvoy_sessions", [], |row| {
        row.get(0)
    })?;
    assert_eq!(count, SESSION_CAPACITY - 256 + 1);
    db.execute("DELETE FROM _kenvoy_sessions", [])?;
    let transaction = db.transaction()?;
    for index in 0..SESSION_CAPACITY {
        transaction.execute(
            "INSERT INTO _kenvoy_sessions(token_hash,expires_at,created_at) VALUES(?1,200,1)",
            [format!("{index:064x}")],
        )?;
    }
    transaction.commit()?;
    assert!(matches!(
        rotate(&mut db, None, &old, None, 100, 200),
        Err(DatabaseError::Auth(StoreError::SessionCapacity))
    ));
    Ok(())
}

#[test]
fn expiration_boundary_and_user_initialization_are_transactional() -> TestResult {
    let mut db = database()?;
    assert!(matches!(
        add_user(&mut db, "user", "phc", Role::User, false, 1),
        Err(DatabaseError::Auth(StoreError::NotInitialized))
    ));
    let admin = add_user(&mut db, "admin", "phc", Role::Admin, true, 1)?;
    assert!(matches!(
        add_user(&mut db, "second", "phc", Role::Admin, true, 1),
        Err(DatabaseError::Auth(StoreError::AlreadyInitialized))
    ));
    assert!(matches!(
        add_user(&mut db, "admin", "phc", Role::User, false, 1),
        Err(DatabaseError::Auth(StoreError::UsernameExists))
    ));
    let token = "c".repeat(64);
    rotate(&mut db, None, &token, Some(admin.id), 100, 101)?;
    assert!(lookup(&db, &token, 100)?.is_some());
    assert!(lookup(&db, &token, 101)?.is_none());
    assert!(matches!(
        rotate(&mut db, Some(&token), &"d".repeat(64), None, 101, 102),
        Err(DatabaseError::Auth(StoreError::InvalidSession))
    ));
    Ok(())
}

#[test]
fn expired_user_sessions_do_not_block_login_behind_global_cleanup_backlog() -> TestResult {
    let mut db = database()?;
    let user = add_user(&mut db, "admin", "phc", Role::Admin, true, 1)?;
    let transaction = db.transaction()?;
    for index in 0..300 {
        transaction.execute(
            "INSERT INTO _kenvoy_sessions(token_hash,expires_at,created_at) VALUES(?1,10,1)",
            [format!("{index:064x}")],
        )?;
    }
    for index in 300..332 {
        transaction.execute("INSERT INTO _kenvoy_sessions(token_hash,user_id,expires_at,created_at) VALUES(?1,?2,20,1)",params![format!("{index:064x}"),user.id])?;
    }
    transaction.commit()?;
    let session = rotate(&mut db, None, &"e".repeat(64), Some(user.id), 100, 200)?;
    assert_eq!(session.user, Some(user));
    Ok(())
}
