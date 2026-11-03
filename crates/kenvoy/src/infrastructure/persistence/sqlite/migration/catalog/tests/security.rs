use super::*;

#[test]
fn sqlite_authorizer_rejects_host_mutation_transactions_and_attachments() -> TestResult {
    let fixture = Fixture::new()?;
    let app = app();
    fixture.seal(&app, None)?;
    for forbidden in [
        "BEGIN; UPDATE items SET age=1; COMMIT;",
        "PRAGMA writable_schema=ON;",
        "ATTACH DATABASE ':memory:' AS outside;",
        "CREATE TABLE _kenvoy_other(value TEXT);",
        "UPDATE _kenvoy_schema SET version=0;",
        "ALTER TABLE items RENAME TO _kenvoy_users;",
        "SELECT load_extension('untrusted');",
    ] {
        let rules = Rules {
            after_sql: vec![forbidden.into()],
            ..Rules::default()
        };
        let path = fixture.root.join("forbidden.json");
        fs::write(&path, serde_json::to_vec(&rules)?)?;
        assert!(
            generate(&fixture.root, &app, "禁止越权 SQL", Some(&path)).is_err(),
            "SQL 不应通过授权：{forbidden}"
        );
        assert_eq!(read(&fixture.root)?.steps.len(), 1);
    }
    Ok(())
}

#[test]
fn sqlite_parser_preserves_semicolons_inside_string_literals() -> TestResult {
    let fixture = Fixture::new()?;
    let app = app();
    fixture.seal(&app, None)?;
    let rules = Rules {
        after_sql: vec![
            "INSERT INTO items(name,age) VALUES('值;不是分隔符',7); UPDATE items SET age=age+1;"
                .into(),
        ],
        ..Rules::default()
    };
    let catalog = fixture.seal(&app, Some(&rules))?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    let row: (String, i64) = connection.query_row("SELECT name,age FROM items", [], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    assert_eq!(row, ("值;不是分隔符".into(), 8));
    Ok(())
}

#[test]
fn tampered_executor_target_or_risk_level_fails_even_with_recomputed_checksum() -> TestResult {
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut executor = catalog.clone();
    executor.steps[0].executor_format = 9;
    executor.steps[0].checksum = executor.steps[0].checksum()?;
    assert!(executor.validate().is_err());
    let mut target = catalog.clone();
    target.steps[0]
        .rules
        .after_sql
        .push("ALTER TABLE items ADD COLUMN unauthorized TEXT".into());
    target.steps[0].risks = super::super::validate::risks(
        &SchemaModel::default(),
        &target.steps[0].schema,
        &target.steps[0].rules,
    )?;
    target.steps[0].risk_level = RiskLevel::Data;
    target.steps[0].checksum = target.steps[0].checksum()?;
    assert!(target.validate().is_err());
    let mut level = catalog.clone();
    level.steps[0].risk_level = RiskLevel::Data;
    level.steps[0].checksum = level.steps[0].checksum()?;
    assert!(level.validate().is_err());
    Ok(())
}

#[test]
fn malicious_expressions_cannot_escape_copy_or_commit_partial_history() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    let initial = fixture.seal(&app, None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &initial)?;
    connection.execute("INSERT INTO items(name,age) VALUES('保留',12)", [])?;
    app.resources[0].fields.push(field("note", FieldKind::Text));
    for expression in [
        "name) FROM items; COMMIT; BEGIN; SELECT (name",
        "name) FROM items; PRAGMA writable_schema=ON; SELECT (name",
    ] {
        let rules = Rules {
            backfills: vec![Expression {
                table: "items".into(),
                column: "note".into(),
                expression: expression.into(),
            }],
            ..Rules::default()
        };
        let path = fixture.root.join("malicious.json");
        fs::write(&path, serde_json::to_vec(&rules)?)?;
        assert!(generate(&fixture.root, &app, "恶意转换", Some(&path)).is_err());
        let mut draft = initial.steps[0].clone();
        draft.id = "000002".into();
        draft.previous = Some("000001".into());
        draft.from_hash = draft.to_hash.clone();
        draft.schema = schema_model(&app)?;
        draft.to_hash = schema_hash(&draft.schema)?;
        draft.rules = rules;
        draft.risks =
            super::super::validate::risks(&initial.steps[0].schema, &draft.schema, &draft.rules)?;
        draft.risk_level = super::super::model::risk_level(&draft.risks);
        draft.checksum = draft.checksum()?;
        fs::write(
            fixture.root.join("migrations/draft.json"),
            serde_json::to_vec(&draft)?,
        )?;
        assert!(seal(&fixture.root, &app, Some(&draft.checksum)).is_err());
        assert_eq!(
            connection.query_row("SELECT age FROM items", [], |row| row.get::<_, i64>(0))?,
            12
        );
        assert_eq!(
            history(&fixture.path)?["applied"]
                .as_array()
                .ok_or("缺少审计")?
                .len(),
            1
        );
        assert!(connection.is_autocommit());
    }
    Ok(())
}

#[test]
fn baseline_rejects_data_violating_frozen_constraints_despite_matching_schema() -> TestResult {
    let fixture = Fixture::new()?;
    let mut app = app();
    app.resources[0].fields[1].min = Some(0.0);
    let catalog = fixture.seal(&app, None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    connection.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO items(name,age) VALUES('非法',-1); PRAGMA ignore_check_constraints=OFF; DROP TABLE _kenvoy_catalog_applied; DROP TABLE _kenvoy_catalog_baseline; DROP TABLE _kenvoy_catalog_owner;")?;
    let outcome = baseline(&fixture.path, &catalog, "000001", None, None);
    assert!(outcome.is_err(), "非法记录应阻断接入：{outcome:?}");
    connection.execute("UPDATE items SET age=1", [])?;
    assert_eq!(
        baseline(&fixture.path, &catalog, "000001", None, None)?["status"],
        "preview"
    );
    Ok(())
}

#[test]
fn invalid_utf8_data_migration_keeps_records_readable_and_history_uncommitted() -> TestResult {
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    connection.execute("INSERT INTO items(name,age) VALUES('可读取',7)", [])?;
    let catalog = fixture.seal(
        &app(),
        Some(&Rules {
            after_sql: vec!["UPDATE items SET name=CAST(X'80' AS TEXT)".into()],
            ..Rules::default()
        }),
    )?;
    let outcome = fixture.apply(&mut connection, &catalog);
    assert!(
        outcome.is_err(),
        "不允许提交消费者无法解码的文本迁移：{outcome:?}"
    );
    assert_eq!(
        connection.query_row("SELECT name FROM items", [], |row| row.get::<_, String>(0))?,
        "可读取"
    );
    assert_eq!(
        history(&fixture.path)?["applied"]
            .as_array()
            .ok_or("缺少真实迁移历史")?
            .len(),
        1
    );
    assert_eq!(status(&fixture.path, &catalog)?["position"]["id"], "000001");
    Ok(())
}

#[test]
fn current_database_still_verifies_system_columns_without_pending_steps() -> TestResult {
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &catalog)?;
    connection.execute(
        "INSERT INTO items(id,name,age) VALUES(9007199254740992,'不可安全编码',1)",
        [],
    )?;
    assert!(status(&fixture.path, &catalog).is_err());
    assert!(apply(&mut connection, &fixture.path, &catalog, None).is_err());
    Ok(())
}

#[test]
fn executor_cli_requirement_uses_semver_precedence_and_rejects_prerelease_contracts() -> TestResult
{
    let fixture = Fixture::new()?;
    let catalog = fixture.seal(&app(), None)?;
    let mut build = catalog.clone();
    build.steps[0].min_cli = format!("{}+catalog.1", env!("CARGO_PKG_VERSION"));
    build.steps[0].checksum = build.steps[0].checksum()?;
    build.validate()?;
    let mut prerelease = catalog;
    prerelease.steps[0].min_cli = "0.2.1-rc.1".into();
    prerelease.steps[0].checksum = prerelease.steps[0].checksum()?;
    assert!(prerelease.validate().is_err());
    Ok(())
}
