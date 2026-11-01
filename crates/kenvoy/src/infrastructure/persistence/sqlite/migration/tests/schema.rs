use super::*;

#[test]
fn legacy_scalar_schema_keeps_identical_serialization_and_hash() -> TestResult {
    let legacy = r#"{"tables":{"items":{"columns":{"note":{"name":"note","kind":"text","required":false,"integer":false,"min":null,"max":null,"max_length":null,"options":[]}},"indexes":{}}}}"#;
    let model: SchemaModel = serde_json::from_str(legacy)?;
    assert_eq!(serde_json::to_string(&model)?, legacy);
    let expected_hash: String = Sha256::digest(legacy.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(schema_hash(&model)?, expected_hash);
    assert_eq!(model.tables["items"].columns["note"].upload, None);
    Ok(())
}

#[test]
fn untrusted_ir_cannot_bypass_file_schema_and_default_restrictions() -> TestResult {
    let mut original = app();
    original.resources[0]
        .fields
        .push(field("attachments", FieldKind::File));
    assert!(schema_model(&original).is_ok());
    for change in 0..9 {
        let mut invalid = original.clone();
        let file = &mut invalid.resources[0].fields[3];
        match change {
            0 => file.upload = None,
            1 => file.unique = true,
            2 => file.index = true,
            3 => file.min = Some(0.0),
            4 => file.max_length = Some(10),
            5 => {
                file.default = Some(serde_json::json!([{
                    "id":"a".repeat(64),"name":"文件","content_type":"application/pdf","size":1
                }]))
            }
            6 => file.upload.as_mut().ok_or("缺少上传约束")?.max_files = 21,
            7 => file.upload.as_mut().ok_or("缺少上传约束")?.accept = vec![".png".into()],
            8 => {
                file.required = true;
                file.default = Some(serde_json::json!([]));
            }
            _ => unreachable!(),
        }
        assert!(schema_model(&invalid).is_err(), "{change}");
    }
    let mut invalid = app();
    invalid.resources[0].fields[0].upload = Some(FileConstraints::default());
    assert!(schema_model(&invalid).is_err());
    Ok(())
}

#[test]
fn schema_and_metadata_drift_and_unmanaged_objects_are_detected() -> TestResult {
    for sql in [
        "ALTER TABLE customers ADD COLUMN manual TEXT;",
        "CREATE INDEX manual_index ON customers(name);",
        "CREATE TABLE _kenvoy_unknown(value TEXT);",
        "CREATE TABLE sqlitepeople(value TEXT);",
        "UPDATE _kenvoy_migrations SET checksum='tampered';",
        "ALTER TABLE _kenvoy_schema ADD COLUMN injected TEXT;",
        "CREATE TRIGGER injected AFTER UPDATE ON _kenvoy_schema BEGIN DELETE FROM customers; END;",
    ] {
        let fixture = DatabaseFile::new()?;
        let mut connection = fixture.connect()?;
        fixture.apply(&mut connection, &app())?;
        connection.execute_batch(sql)?;
        assert!(
            matches!(
                preview(&fixture.path, &app()),
                Err(MigrationError::Drift(_))
            ),
            "必须检测漂移：{sql}"
        );
    }
    let fixture = DatabaseFile::new()?;
    let connection = fixture.connect()?;
    connection.execute_batch("CREATE TABLE existing(value TEXT);")?;
    assert!(matches!(
        preview(&fixture.path, &app()),
        Err(MigrationError::Unmanaged(_))
    ));
    Ok(())
}

#[test]
fn model_validation_and_sql_constraints_reject_untrusted_artifacts_and_values() -> TestResult {
    let mut application = app();
    application.resources[0].name = "customers; DROP TABLE other".into();
    assert!(schema_model(&application).is_err());
    application = app();
    application.resources[0].fields[0].name = "id".into();
    assert!(schema_model(&application).is_err());
    application = app();
    application.resources[0].fields[0].default = Some(Value::Bool(true));
    assert!(schema_model(&application).is_err());
    application = app();
    application.resources[0].fields[2].min = Some(0.1);
    application.resources[0].fields[2].max = Some(0.9);
    assert!(schema_model(&application).is_err());
    application = app();
    application.resources[0].fields[0].max_length = Some(1_000_001);
    assert!(schema_model(&application).is_err());
    application = app();
    application.resources[0].fields.clear();
    assert!(schema_model(&application).is_err());
    let fixture = DatabaseFile::new()?;
    let mut connection = fixture.connect()?;
    fixture.apply(&mut connection, &app())?;
    for sql in [
        "INSERT INTO customers(name,age) VALUES('小数',1.2)",
        "INSERT INTO customers(name,age) VALUES('大整数',9007199254740992)",
        "INSERT INTO customers(name) VALUES('')",
        "INSERT INTO customers(name,age) VALUES('注入','1;DROP TABLE customers')",
    ] {
        assert!(connection.execute(sql, []).is_err());
    }
    assert_eq!(rows(&connection)?, 0);
    assert_eq!(quote_identifier("name")?, "\"name\"");
    assert!(quote_identifier("bad\"name").is_err());
    Ok(())
}

#[test]
fn required_additions_type_changes_and_dropped_indexes_are_detected() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut connection = fixture.connect()?;
    let mut original = app();
    original.resources[0].fields[2].index = true;
    fixture.apply(&mut connection, &original)?;
    connection.execute("INSERT INTO customers(name,age) VALUES('原有',25)", [])?;
    let mut target = original.clone();
    let mut required = field("extra", FieldKind::Text);
    required.required = true;
    target.resources[0].fields.push(required);
    assert!(preview(&fixture.path, &target)?.destructive);
    assert!(matches!(
        fixture.confirm(&mut connection, &target),
        Err(MigrationError::InvalidData(_))
    ));
    let mut target = original.clone();
    target.resources[0].fields[1].kind = FieldKind::Text;
    target.resources[0].fields[1].integer = false;
    assert!(
        preview(&fixture.path, &target)?
            .changes
            .iter()
            .any(|change| matches!(change,SchemaChange::ChangeColumn {column,..} if column=="age"))
    );
    assert!(
        matches!(
            fixture.confirm(&mut connection, &target),
            Err(MigrationError::InvalidData(_))
        ),
        "确认不代表允许隐式类型转换"
    );
    let mut target = original.clone();
    target.resources[0].fields[2].index = false;
    assert!(preview(&fixture.path, &target)?.destructive);
    assert!(fixture.confirm(&mut connection, &target)?.applied);
    assert_eq!(rows(&connection)?, 1);
    assert_eq!(history(&connection)?.len(), 2);
    Ok(())
}
