use super::*;

#[test]
fn explicit_rename_backfill_and_conversion_preserve_rows_and_sequence() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    let initial = fixture.seal(&app, None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &initial)?;
    connection.execute("INSERT INTO items(id,name,age) VALUES(90,'旧值',42)", [])?;
    connection.execute("DELETE FROM items WHERE id=90", [])?;
    connection.execute("INSERT INTO items(name,age) VALUES('原始',42)", [])?;
    app.resources[0].fields[0].name = "title".into();
    app.resources[0].fields[1].kind = FieldKind::Text;
    app.resources[0].fields[1].integer = false;
    let mut required = field("required_name", FieldKind::Text);
    required.required = true;
    app.resources[0].fields.push(required);
    let rules = Rules {
        renames: vec![Rename {
            table: "items".into(),
            from: "name".into(),
            to: "title".into(),
        }],
        backfills: vec![Expression {
            table: "items".into(),
            column: "required_name".into(),
            expression: "name || ';回填'".into(),
        }],
        conversions: vec![Expression {
            table: "items".into(),
            column: "age".into(),
            expression: "CAST(age AS TEXT)".into(),
        }],
        ..Rules::default()
    };
    let catalog = fixture.seal(&app, Some(&rules))?;
    fixture.apply(&mut connection, &catalog)?;
    let row: (i64, String, String, String) =
        connection.query_row("SELECT id,title,age,required_name FROM items", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
    assert_eq!(row, (91, "原始".into(), "42".into(), "原始;回填".into()));
    connection.execute(
        "INSERT INTO items(title,required_name) VALUES('新值','合法')",
        [],
    )?;
    assert_eq!(connection.last_insert_rowid(), 92);
    Ok(())
}

#[test]
fn destructive_draft_never_guesses_or_seals_without_resolved_risks() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    fixture.seal(&app, None)?;
    app.resources[0].fields[0].name = "title".into();
    let draft = generate(&fixture.root, &app, "未映射改名", None)?;
    assert!(seal(&fixture.root, &app, draft["checksum"].as_str()).is_err());
    let rules = Rules {
        renames: vec![Rename {
            table: "items".into(),
            from: "name".into(),
            to: "title".into(),
        }],
        ..Rules::default()
    };
    let rules_path = fixture.root.join("rename.json");
    fs::write(&rules_path, serde_json::to_vec(&rules)?)?;
    let mapped = generate(&fixture.root, &app, "明确改名", Some(&rules_path))?;
    assert!(matches!(
        seal(&fixture.root, &app, None),
        Err(MigrationError::ConfirmationRequired { .. })
    ));
    seal(&fixture.root, &app, mapped["checksum"].as_str())?;
    let catalog = read(&fixture.root)?;
    let mut connection = fixture.connect()?;
    assert!(matches!(
        apply(&mut connection, &fixture.path, &catalog, None),
        Err(MigrationError::ConfirmationRequired { .. })
    ));
    assert!(!super::super::super::state::table_exists(
        &connection,
        "items"
    )?);
    Ok(())
}
