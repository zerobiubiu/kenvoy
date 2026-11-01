use super::*;

#[test]
fn registered_file_schema_never_recreates_completely_missing_upload_storage() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut connection = fixture.connect()?;
    let mut application = app();
    application.resources[0]
        .fields
        .push(field("attachments", FieldKind::File));
    fixture.apply(&mut connection, &application)?;
    assert!(crate::infrastructure::persistence::sqlite::uploads::validate_schema(&connection)?);
    connection.execute_batch("DROP TABLE _kenvoy_uploads; DROP TABLE _kenvoy_upload_schema;")?;
    assert!(matches!(
        preview(&fixture.path, &application),
        Err(MigrationError::Drift(_))
    ));
    assert!(matches!(
        history_path(&fixture.path),
        Err(MigrationError::Drift(_))
    ));
    assert!(matches!(
        fixture.apply(&mut connection, &application),
        Err(MigrationError::Drift(_))
    ));
    assert!(!table_exists(&connection, "_kenvoy_uploads")?);
    assert!(!table_exists(&connection, "_kenvoy_upload_schema")?);
    Ok(())
}

#[test]
fn file_json_text_survives_rebuilds_and_rejected_constraints_preserve_data() -> TestResult {
    let fixture = DatabaseFile::new()?;
    let mut connection = fixture.connect()?;
    let mut original = app();
    let mut attachments = field("attachments", FieldKind::File);
    attachments.default = Some(serde_json::json!([]));
    attachments.upload = Some(FileConstraints {
        accept: vec!["image/*".into()],
        max_size: 10,
        max_files: 2,
    });
    original.resources[0].fields.push(attachments);
    fixture.apply(&mut connection, &original)?;
    let payload = serde_json::json!([
        {"id":"a".repeat(64),"name":"甲.png","content_type":"image/png","size":2},
        {"id":"b".repeat(64),"name":"乙.webp","content_type":"image/webp","size":3}
    ])
    .to_string();
    connection.execute(
        "INSERT INTO customers(name,phone,attachments) VALUES('有附件','[]',?1)",
        [&payload],
    )?;
    connection.execute("INSERT INTO customers(name) VALUES('空附件')", [])?;
    let (storage, default): (String, String) = connection.query_row(
        "SELECT typeof(attachments),attachments FROM customers WHERE name='空附件'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(storage, "text");
    assert_eq!(default, "[]");
    for invalid in ["{}", "null", "\"[]\"", "[", "[{},{},{}]"] {
        assert!(
            connection
                .execute(
                    "INSERT INTO customers(name,attachments) VALUES('非法附件',?1)",
                    [invalid],
                )
                .is_err(),
            "{invalid}"
        );
    }
    let mut rebuilt = original.clone();
    rebuilt.resources[0].fields[2].max_length = Some(100);
    assert!(fixture.confirm(&mut connection, &rebuilt)?.applied);
    drop(connection);
    let mut connection = fixture.connect()?;
    assert_eq!(
        connection.query_row::<String, _, _>(
            "SELECT attachments FROM customers WHERE name='有附件'",
            [],
            |row| row.get(0)
        )?,
        payload
    );
    let mut too_few = rebuilt.clone();
    too_few.resources[0].fields[3]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .max_files = 1;
    let mut too_small = rebuilt.clone();
    too_small.resources[0].fields[3]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .max_size = 1;
    let mut incompatible_type = rebuilt.clone();
    incompatible_type.resources[0].fields[3]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .accept = vec!["application/pdf".into()];
    let mut required = rebuilt.clone();
    required.resources[0].fields[3].required = true;
    required.resources[0].fields[3].default = None;
    let mut file_to_text = rebuilt.clone();
    file_to_text.resources[0].fields[3].kind = FieldKind::Text;
    file_to_text.resources[0].fields[3].upload = None;
    file_to_text.resources[0].fields[3].default = None;
    let mut text_to_file = rebuilt.clone();
    text_to_file.resources[0].fields[2].kind = FieldKind::File;
    text_to_file.resources[0].fields[2].max_length = None;
    text_to_file.resources[0].fields[2].upload = Some(FileConstraints::default());
    for invalid in [
        too_few,
        too_small,
        incompatible_type,
        required,
        file_to_text,
        text_to_file,
    ] {
        assert!(matches!(
            fixture.confirm(&mut connection, &invalid),
            Err(MigrationError::InvalidData(_))
        ));
        assert_eq!(rows(&connection)?, 2);
        assert_eq!(history(&connection)?.len(), 2);
        assert!(preview(&fixture.path, &rebuilt)?.changes.is_empty());
        assert_eq!(
            connection.query_row::<String, _, _>(
                "SELECT attachments FROM customers WHERE name='有附件'",
                [],
                |row| row.get(0)
            )?,
            payload
        );
    }
    Ok(())
}
