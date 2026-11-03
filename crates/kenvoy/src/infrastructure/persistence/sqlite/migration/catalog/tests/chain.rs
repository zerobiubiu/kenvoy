use super::*;

#[test]
fn complete_chain_data_step_and_idempotent_replay() -> TestResult {
    let fixture = Fixture::new()?;
    let app = app();
    fixture.seal(&app, None)?;
    let rules = Rules {
        after_sql: vec!["UPDATE items SET age=age+1".into()],
        assertions: vec![assertion("SELECT count(*) FROM items WHERE age<0", 0)],
        ..Rules::default()
    };
    let catalog = fixture.seal(&app, Some(&rules))?;
    let mut connection = fixture.connect()?;
    let first = Catalog {
        steps: catalog.steps[..1].to_vec(),
        ..Catalog::default()
    };
    fixture.apply(&mut connection, &first)?;
    connection.execute("INSERT INTO items(name,age) VALUES('保留',40)", [])?;
    fixture.apply(&mut connection, &catalog)?;
    assert_eq!(
        connection.query_row("SELECT age FROM items", [], |row| row.get::<_, i64>(0))?,
        41
    );
    assert_eq!(
        history(&fixture.path)?["applied"]
            .as_array()
            .ok_or("缺少审计")?
            .len(),
        2
    );
    let outcome = fixture.apply(&mut connection, &catalog)?;
    assert_eq!(outcome["status"], "unchanged");
    assert_eq!(
        connection.query_row("SELECT age FROM items", [], |row| row.get::<_, i64>(0))?,
        41
    );
    let fresh = Fixture::new()?;
    let mut empty = fresh.connect()?;
    fresh.apply(&mut empty, &catalog)?;
    assert_eq!(
        history(&fresh.path)?["applied"]
            .as_array()
            .ok_or("缺少审计")?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn failing_later_step_preserves_commits_and_resumes_without_replay() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    fixture.seal(&app, None)?;
    app.resources[0].fields.push(field("note", FieldKind::Text));
    fixture.seal(&app, None)?;
    let rules = Rules {
        after_sql: vec!["UPDATE items SET age=age+1".into()],
        assertions: vec![assertion("SELECT count(*) FROM items WHERE name='阻断'", 0)],
        ..Rules::default()
    };
    let catalog = fixture.seal(&app, Some(&rules))?;
    let mut connection = fixture.connect()?;
    let initial = Catalog {
        steps: catalog.steps[..1].to_vec(),
        ..Catalog::default()
    };
    fixture.apply(&mut connection, &initial)?;
    connection.execute("INSERT INTO items(name,age) VALUES('阻断',5)", [])?;
    let error = fixture
        .apply(&mut connection, &catalog)
        .expect_err("第三步应被数据断言阻止");
    let message = error.to_string();
    assert!(message.contains("000003") && message.contains("000002") && message.contains("断言"));
    let audit = history(&fixture.path)?;
    assert_eq!(audit["applied"].as_array().ok_or("缺少审计")?.len(), 2);
    assert_eq!(
        connection.query_row("SELECT age FROM items", [], |row| row.get::<_, i64>(0))?,
        5
    );
    connection.execute("UPDATE items SET name='恢复'", [])?;
    fixture.apply(&mut connection, &catalog)?;
    assert_eq!(
        connection.query_row("SELECT age FROM items", [], |row| row.get::<_, i64>(0))?,
        6
    );
    assert_eq!(
        history(&fixture.path)?["applied"]
            .as_array()
            .ok_or("缺少审计")?
            .len(),
        3
    );
    Ok(())
}

#[test]
fn changed_missing_duplicate_forked_and_ahead_history_are_rejected() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    let initial = fixture.seal(&app, None)?;
    app.resources[0].fields.push(field("note", FieldKind::Text));
    let catalog = fixture.seal(&app, None)?;
    let mut changed = catalog.clone();
    changed.steps[0].description.push('改');
    assert!(changed.validate().is_err());
    let mut duplicate = catalog.clone();
    duplicate.steps[1].id = duplicate.steps[0].id.clone();
    duplicate.steps[1].checksum = duplicate.steps[1].checksum()?;
    assert!(duplicate.validate().is_err());
    let mut fork = catalog.clone();
    fork.steps[1].previous = None;
    fork.steps[1].checksum = fork.steps[1].checksum()?;
    assert!(fork.validate().is_err());
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    assert!(apply(&mut connection, &fixture.path, &initial, None).is_err());
    fs::remove_file(fixture.root.join("migrations/steps/000001.json"))?;
    assert!(read(&fixture.root).is_err());
    Ok(())
}

#[test]
fn unknown_internal_names_and_tampered_owner_are_not_ignored() -> TestResult {
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    connection.execute_batch("CREATE TABLE _kenvoy_unrecognized(value TEXT)")?;
    assert!(status(&fixture.path, &catalog).is_err());
    connection.execute_batch(
        "DROP TABLE _kenvoy_unrecognized; UPDATE _kenvoy_catalog_owner SET owner='other'",
    )?;
    assert!(status(&fixture.path, &catalog).is_err());
    Ok(())
}
