use super::*;

#[test]
fn baseline_preview_is_readonly_and_never_forges_legacy_or_formal_execution() -> TestResult {
    let fixture = Fixture::new()?;
    let app = app();
    let initial = fixture.seal(&app, None)?;
    let data = Rules {
        after_sql: vec!["UPDATE items SET age=age+1".into()],
        ..Rules::default()
    };
    let catalog = fixture.seal(&app, Some(&data))?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &initial)?;
    connection.execute("INSERT INTO items(name,age) VALUES('旧数据',40)", [])?;
    fixture.apply(&mut connection, &catalog)?;
    let original = history(&fixture.path)?["legacy_history"].clone();
    connection.execute_batch("DROP TABLE _kenvoy_catalog_applied; DROP TABLE _kenvoy_catalog_baseline; DROP TABLE _kenvoy_catalog_owner;")?;
    assert_eq!(
        status(&fixture.path, &catalog)?["status"],
        "baseline_required"
    );
    assert!(apply(&mut connection, &fixture.path, &catalog, None).is_err());
    assert!(baseline(&fixture.path, &catalog, "000002", None, None).is_err());
    let rules = Rules {
        baseline_proofs: vec![BaselineProof {
            step: "000002".into(),
            assertions: vec![assertion("SELECT count(*) FROM items WHERE age<>41", 0)],
        }],
        ..Rules::default()
    };
    let rules_path = fixture.root.join("baseline-rules.json");
    fs::write(&rules_path, serde_json::to_vec(&rules)?)?;
    let preview = baseline(&fixture.path, &catalog, "000002", Some(&rules_path), None)?;
    assert_eq!(preview["status"], "preview");
    assert_eq!(history(&fixture.path)?["legacy_history"], original);
    assert!(!super::super::super::state::table_exists(
        &connection,
        "_kenvoy_catalog_owner"
    )?);
    let adopted = baseline(
        &fixture.path,
        &catalog,
        "000002",
        Some(&rules_path),
        preview["checksum"].as_str(),
    )?;
    assert_eq!(adopted["status"], "baselined");
    let audit = history(&fixture.path)?;
    assert_eq!(audit["legacy_history"], original);
    assert_eq!(audit["applied"], json!([]));
    assert_eq!(audit["baseline"]["at_id"], "000002");
    assert!(
        audit["baseline"]["backup"]
            .as_str()
            .is_some_and(|path| Path::new(path).exists())
    );
    assert_eq!(
        fixture.apply(&mut connection, &catalog)?["status"],
        "unchanged"
    );
    Ok(())
}

#[test]
fn empty_and_development_snapshot_databases_cannot_be_baselined() -> TestResult {
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut connection = fixture.connect()?;
    assert!(baseline(&fixture.path, &catalog, "000001", None, None).is_err());
    let mut marker = fixture.path.as_os_str().to_os_string();
    marker.push(".kenvoy-snapshot.json");
    fs::write(Path::new(&marker), b"{}")?;
    assert!(apply(&mut connection, &fixture.path, &catalog, None).is_err());
    assert!(baseline(&fixture.path, &catalog, "000001", None, None).is_err());
    Ok(())
}

#[test]
fn untracked_but_exact_business_schema_is_adopted_with_real_source() -> TestResult {
    let fixture = Fixture::new()?;
    let app = app();
    let catalog = fixture.seal(&app, None)?;
    let mut connection = fixture.connect()?;
    super::super::super::super::schema::frozen::create(&connection, &catalog.steps[0].schema)?;
    connection.execute("INSERT INTO items(name,age) VALUES('原始',3)", [])?;
    let preview = baseline(&fixture.path, &catalog, "000001", None, None)?;
    baseline(
        &fixture.path,
        &catalog,
        "000001",
        None,
        preview["checksum"].as_str(),
    )?;
    let audit = history(&fixture.path)?;
    assert_eq!(audit["legacy_history"], json!([]));
    assert_eq!(audit["applied"], json!([]));
    assert_eq!(
        fixture.apply(&mut connection, &catalog)?["status"],
        "unchanged"
    );
    Ok(())
}
