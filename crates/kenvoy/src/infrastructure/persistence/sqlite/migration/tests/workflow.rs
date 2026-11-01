use super::*;

#[test]
fn preview_and_history_do_not_create_database_or_directories() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let missing = fixture.directory.join("nested/data.sqlite");
    let plan = preview(&missing, &app())?;
    assert_eq!(plan.from_version, 0);
    assert!(!plan.destructive);
    assert_eq!(plan.changes.len(), 1);
    assert!(history_path(&missing)?.is_empty());
    assert!(!missing.parent().ok_or("缺少父目录")?.exists());
    Ok(())
}

#[test]
fn initialization_persists_data_and_replay_ignores_labels_locations_and_field_order() -> TestResult
{
    let fixture = DatabaseFile::new()?;
    let original = app();
    let expected = preview(&fixture.path, &original)?;
    let mut connection = fixture.connect()?;
    let outcome = fixture.apply(&mut connection, &original)?;
    assert!(outcome.applied);
    assert_eq!(expected.checksum, outcome.plan.checksum);
    connection.execute(
        "INSERT INTO customers(name,age) VALUES(?1,?2)",
        params!["张三", 25],
    )?;
    let timestamp: String =
        connection.query_row("SELECT created_at FROM customers", [], |row| row.get(0))?;
    assert!(timestamp.ends_with('Z'));
    drop(connection);
    let mut connection = fixture.connect()?;
    assert_eq!(rows(&connection)?, 1);
    let mut changed = original.clone();
    changed.name = "新标题".into();
    changed.resources[0].fields[0].label = "新标签".into();
    changed.resources[0].fields.reverse();
    changed.resources[0].source.path = "other.tsx".into();
    assert_eq!(
        schema_hash(&schema_model(&original)?)?,
        schema_hash(&schema_model(&changed)?)?
    );
    assert!(!fixture.apply(&mut connection, &changed)?.applied);
    assert_eq!(history(&connection)?.len(), 1);
    assert_eq!(history_path(&fixture.path)?.len(), 1);
    Ok(())
}

#[test]
fn nullable_and_default_columns_and_indexes_are_safe_real_migrations() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut application = app();
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &application)?;
    connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
    let mut active = field("active", FieldKind::Boolean);
    active.required = true;
    active.default = Some(Value::Bool(true));
    active.index = true;
    application.resources[0].fields.push(active);
    application.resources[0]
        .fields
        .push(field("notes", FieldKind::Textarea));
    application.resources[0].fields[0].unique = true;
    let plan = preview(&fixture.path, &application)?;
    assert!(!plan.destructive);
    assert!(fixture.apply(&mut connection, &application)?.applied);
    let (active, notes): (i64, Option<String>) =
        connection.query_row("SELECT active,notes FROM customers", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    assert_eq!(active, 1);
    assert_eq!(notes, None);
    assert_eq!(history(&connection)?.len(), 2);
    assert!(
        connection
            .execute("INSERT INTO customers(name) VALUES('原有')", [])
            .is_err()
    );
    assert!(table_exists(&connection, "customers")?);
    Ok(())
}

#[test]
fn stale_confirmation_rename_candidates_and_unbacked_changes_are_rejected() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut application = app();
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &application)?;
    application.resources[0].fields[2].name = "mobile".into();
    let pending = preview(&fixture.path, &application)?;
    assert!(pending.changes.iter().any(|change|matches!(change,SchemaChange::RenameCandidate{from,to,..}if from=="phone"&&to=="mobile")));
    assert!(matches!(
        fixture.confirm(&mut connection, &application),
        Err(MigrationError::RenameMappingRequired)
    ));
    assert!(matches!(
        apply_connection(
            &mut connection,
            &application,
            Some(&fixture.path),
            Some("obsolete")
        ),
        Err(MigrationError::StaleConfirmation)
    ));
    let mut memory = Connection::open_in_memory()?;
    let original = app();
    apply_connection(&mut memory, &original, None, None)?;
    let mut changed = original.clone();
    changed.resources[0]
        .fields
        .retain(|field| field.name != "phone");
    let state = read_state(&memory)?;
    let checksum = plan(Path::new(":memory:"), &state, &schema_model(&changed)?)?.checksum;
    assert!(matches!(
        apply_connection(&mut memory, &changed, None, Some(&checksum)),
        Err(MigrationError::BackupRequired)
    ));
    Ok(())
}
