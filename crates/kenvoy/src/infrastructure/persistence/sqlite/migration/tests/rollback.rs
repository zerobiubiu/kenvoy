use super::*;

#[test]
fn uniqueness_preflight_and_failed_ddl_preserve_schema_history_and_data() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut application = app();
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &application)?;
    connection.execute_batch("INSERT INTO customers(name) VALUES('重复'),('重复');")?;
    application.resources[0].fields[0].unique = true;
    assert!(matches!(
        fixture.apply(&mut connection, &application),
        Err(MigrationError::InvalidData(_))
    ));
    assert_eq!(rows(&connection)?, 2);
    assert_eq!(history(&connection)?.len(), 1);
    application.resources[0].fields[0].unique = false;
    // SQLite 的 ALTER TABLE 禁止含非恒定默认表达式；通过失败的语句注入验证整个事务回滚。
    let before = actual_objects(&connection)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch("ALTER TABLE customers ADD COLUMN first TEXT;")?;
    assert!(
        transaction
            .execute_batch("CREATE UNIQUE INDEX broken_unique ON customers(name);")
            .is_err()
    );
    drop(transaction);
    assert_eq!(actual_objects(&connection)?, before);
    assert_eq!(history(&connection)?.len(), 1);
    assert!(preview(&fixture.path, &application)?.changes.is_empty());
    Ok(())
}

#[test]
fn destructive_confirmation_backup_rebuild_and_sequence_high_water_are_preserved() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut application = app();
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &application)?;
    connection.execute_batch("INSERT INTO customers(id,name,age,phone) VALUES(1,'保留',25,'13800000000'),(40,'已删除',30,'13900000000'); DELETE FROM customers WHERE id=40;")?;
    application.resources[0]
        .fields
        .retain(|field| field.name != "phone");
    assert!(matches!(
        fixture.apply(&mut connection, &application),
        Err(MigrationError::ConfirmationRequired { .. })
    ));
    assert_eq!(history(&connection)?.len(), 1);
    assert_eq!(rows(&connection)?, 1);
    let outcome = fixture.confirm(&mut connection, &application)?;
    let backup = outcome.backup.ok_or("缺少备份")?;
    assert!(
        backup.starts_with(fixture.directory.join("backups"))
            || backup
                .canonicalize()?
                .starts_with(fixture.directory.canonicalize()?.join("backups"))
    );
    let saved = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let phone: String = saved.query_row("SELECT phone FROM customers WHERE id=1", [], |row| {
        row.get(0)
    })?;
    assert_eq!(phone, "13800000000");
    let fields: Vec<String> = connection
        .prepare("PRAGMA table_info(customers)")?
        .query_map([], |row| row.get(1))?
        .collect::<std::result::Result<_, _>>()?;
    assert!(!fields.iter().any(|name| name == "phone"));
    connection.execute("INSERT INTO customers(name) VALUES('后来')", [])?;
    assert_eq!(connection.last_insert_rowid(), 41);
    assert_eq!(history(&connection)?.len(), 2);
    assert!(!fixture.apply(&mut connection, &application)?.applied);
    Ok(())
}

#[test]
fn constraint_changes_validate_existing_values_and_roll_back_then_recover() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut application = app();
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &application)?;
    connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
    application.resources[0].fields[1].min = Some(30.0);
    assert!(matches!(
        fixture.confirm(&mut connection, &application),
        Err(MigrationError::InvalidData(_))
    ));
    assert_eq!(history(&connection)?.len(), 1);
    assert_eq!(rows(&connection)?, 1);
    application.resources[0].fields[1].min = Some(20.0);
    assert!(fixture.confirm(&mut connection, &application)?.applied);
    assert!(
        connection
            .execute("INSERT INTO customers(name,age) VALUES('不符',19)", [])
            .is_err()
    );
    assert_eq!(rows(&connection)?, 1);
    Ok(())
}
