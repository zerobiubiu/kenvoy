use super::*;
use crate::{
    application::{ApplicationIR, FileConstraints},
    dsl::SourceLocation,
};
use serde_json::json;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn resource() -> ResourceDefinition {
    let source = SourceLocation {
        path: "test.tsx".into(),
        span: kengine::SourceSpan {
            start: 0,
            end: 0,
            start_position: kengine::Position { line: 1, column: 1 },
            end_position: kengine::Position { line: 1, column: 1 },
        },
    };
    ResourceDefinition {
        name: "customers".into(),
        fields: vec![FieldDefinition {
            name: "attachments".into(),
            label: "附件".into(),
            kind: FieldKind::File,
            required: false,
            default: None,
            integer: false,
            min: None,
            max: None,
            max_length: None,
            unique: false,
            index: false,
            options: vec![],
            options_source: None,
            upload: Some(FileConstraints::default()),
            source: source.clone(),
        }],
        source,
    }
}

fn application(resource: ResourceDefinition) -> ApplicationIR {
    ApplicationIR {
        format_version: crate::application::APPLICATION_IR_VERSION,
        name: "附件数据库".into(),
        resources: vec![resource],
        pages: vec![],
        actions: vec![],
        navigation: vec![],
        permissions: vec![],
        module_references: vec![],
        component_overrides: Default::default(),
    }
}

fn upload(owner: char, expires_at: i64) -> NewUpload {
    NewUpload {
        resource: "customers".into(),
        field: "attachments".into(),
        owner: owner.to_string().repeat(64),
        name: "文档.pdf".into(),
        content_type: "application/pdf".into(),
        data: b"%PDF-1.7".to_vec(),
        expires_at,
    }
}

fn count(connection: &Connection) -> TestResult<i64> {
    Ok(connection.query_row("SELECT count(*) FROM _kenvoy_uploads", [], |row| row.get(0))?)
}

#[test]
fn fixed_upload_schema_rejects_partial_ddl_index_trigger_and_version_drift() -> TestResult {
    for tamper in [
        "DROP TABLE _kenvoy_uploads",
        "ALTER TABLE _kenvoy_uploads ADD COLUMN injected TEXT",
        "DROP INDEX _kenvoy_uploads_owner",
        "CREATE TRIGGER injected AFTER INSERT ON _kenvoy_uploads BEGIN DELETE FROM _kenvoy_uploads; END",
        "DELETE FROM _kenvoy_upload_schema",
    ] {
        let mut connection = Connection::open_in_memory()?;
        assert!(!validate_schema(&connection)?);
        initialize(&mut connection, 100)?;
        assert!(validate_schema(&connection)?);
        connection.execute_batch(tamper)?;
        assert!(matches!(
            initialize(&mut connection, 100),
            Err(DatabaseError::Migration(migration::MigrationError::Drift(
                _
            )))
        ));
    }
    Ok(())
}

#[test]
fn upload_metadata_and_image_mime_are_checked_against_real_bytes() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    let resource = resource();
    for (mime, bytes) in [
        ("image/png", b"\x89PNG\r\n\x1a\n".as_slice()),
        ("image/jpeg", b"\xff\xd8\xff".as_slice()),
        ("image/gif", b"GIF89a".as_slice()),
        ("image/webp", b"RIFF\x04\0\0\0WEBP".as_slice()),
    ] {
        let mut input = upload('a', 200);
        input.content_type = mime.into();
        input.data = bytes.to_vec();
        let file = stage(&mut connection, &resource, input, 100)?;
        assert!(validation::valid_upload_id(&file.id));
        assert_eq!(file.content_type, mime);
        assert_eq!(content(&connection, &file.id, 100)?, Some(bytes.to_vec()));
        assert!(delete_staged(&connection, &file.id, &"a".repeat(64))?);
    }
    let mut octet_stream = upload('a', 200);
    octet_stream.content_type = "application/octet-stream".into();
    octet_stream.data = b"\x89PNG\r\n\x1a\n".to_vec();
    let file = stage(&mut connection, &resource, octet_stream, 100)?;
    assert_eq!(file.content_type, "image/png");
    for (mime, data) in [
        ("image/png", b"<script>alert(1)</script>".as_slice()),
        ("image/jpeg", b"\x89PNG\r\n\x1a\n".as_slice()),
        ("image/svg+xml", b"<svg/>".as_slice()),
        ("image/avif", b"\0\0\0\x18ftypavif".as_slice()),
        ("Image/PNG", b"\x89PNG\r\n\x1a\n".as_slice()),
        ("application/pdf; charset=utf-8", b"%PDF".as_slice()),
    ] {
        let mut input = upload('a', 200);
        input.content_type = mime.into();
        input.data = data.to_vec();
        assert!(matches!(
            stage(&mut connection, &resource, input, 100),
            Err(DatabaseError::Validation(_))
        ));
    }
    for name in ["", ".", "..", "a/b.pdf", "a\\b.pdf", "a\nb.pdf", "a\0b.pdf"] {
        let mut input = upload('a', 200);
        input.name = name.into();
        assert!(matches!(
            stage(&mut connection, &resource, input, 100),
            Err(DatabaseError::Validation(_))
        ));
    }
    let mut input = upload('a', 200);
    input.name = "文".repeat(256);
    assert!(matches!(
        stage(&mut connection, &resource, input, 100),
        Err(DatabaseError::Validation(_))
    ));
    let mut image_only = resource.clone();
    image_only.fields[0]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .accept = vec!["image/*".into()];
    assert!(matches!(
        stage(&mut connection, &image_only, upload('a', 200), 100),
        Err(DatabaseError::Validation(_))
    ));
    image_only.fields[0]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .max_size = 1;
    let mut input = upload('a', 200);
    input.data = b"GIF89a".to_vec();
    input.content_type = "image/gif".into();
    assert!(matches!(
        stage(&mut connection, &image_only, input, 100),
        Err(DatabaseError::Validation(_))
    ));
    let mut empty = upload('a', 200);
    empty.data.clear();
    assert!(matches!(
        stage(&mut connection, &resource, empty, 100),
        Err(DatabaseError::Validation(_))
    ));
    let mut owner = upload('a', 200);
    owner.owner = "cookie原文".into();
    assert!(matches!(
        stage(&mut connection, &resource, owner, 100),
        Err(DatabaseError::Validation(_))
    ));
    assert!(matches!(
        stage(&mut connection, &resource, upload('a', 100), 100),
        Err(DatabaseError::Validation(_))
    ));
    Ok(())
}

#[test]
fn session_quota_cancel_and_expiration_are_enforced_with_a_fixed_clock() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    let resource = resource();
    let mut files = Vec::new();
    for _ in 0..SESSION_FILES {
        files.push(stage(&mut connection, &resource, upload('a', 200), 100)?);
    }
    assert!(matches!(
        stage(&mut connection, &resource, upload('a', 200), 100),
        Err(DatabaseError::UploadLimit)
    ));
    assert!(!delete_staged(&connection, &files[0].id, &"b".repeat(64))?);
    assert!(delete_staged(&connection, &files[0].id, &"a".repeat(64))?);
    stage(&mut connection, &resource, upload('a', 200), 100)?;
    assert_eq!(count(&connection)?, SESSION_FILES);
    assert!(info(&connection, &files[1].id, 200)?.is_none());
    assert!(content(&connection, &files[1].id, 200)?.is_none());
    let fresh = stage(&mut connection, &resource, upload('a', 300), 200)?;
    assert_eq!(count(&connection)?, 1);
    assert!(info(&connection, &fresh.id, 200)?.is_some());
    initialize(&mut connection, 300)?;
    assert_eq!(count(&connection)?, 0);
    Ok(())
}

#[test]
fn bound_uploads_are_outside_staging_limits_and_do_not_expire() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    let mut resource = resource();
    resource.fields[0]
        .upload
        .as_mut()
        .ok_or("缺少上传约束")?
        .max_files = 20;
    let mut files = Vec::new();
    for _ in 0..SESSION_FILES {
        files.push(stage(&mut connection, &resource, upload('a', 200), 100)?);
    }
    let values = json!({"attachments":files})
        .as_object()
        .ok_or("缺少附件字段")?
        .clone();
    let transaction = connection.transaction()?;
    synchronize(
        &transaction,
        &resource,
        1,
        None,
        &values,
        Some(&"a".repeat(64)),
        100,
    )?;
    transaction.commit()?;
    for _ in 0..SESSION_FILES {
        stage(&mut connection, &resource, upload('a', 200), 100)?;
    }
    assert!(matches!(
        stage(&mut connection, &resource, upload('a', 200), 100),
        Err(DatabaseError::UploadLimit)
    ));
    initialize(&mut connection, 200)?;
    assert_eq!(count(&connection)?, SESSION_FILES);
    for file in files {
        assert_eq!(
            info(&connection, &file.id, 300)?
                .ok_or("缺少已绑定附件")?
                .record_id,
            Some(1)
        );
        assert!(!delete_staged(&connection, &file.id, &"a".repeat(64))?);
    }
    Ok(())
}

#[test]
fn byte_quota_boundaries_and_global_file_count_are_enforced() -> TestResult {
    assert!(!quota_full(
        0,
        SESSION_BYTES - 8,
        8,
        SESSION_FILES,
        SESSION_BYTES
    ));
    assert!(quota_full(
        0,
        SESSION_BYTES - 7,
        8,
        SESSION_FILES,
        SESSION_BYTES
    ));
    assert!(!quota_full(
        0,
        GLOBAL_BYTES - 8,
        8,
        GLOBAL_FILES,
        GLOBAL_BYTES
    ));
    assert!(quota_full(
        0,
        GLOBAL_BYTES - 7,
        8,
        GLOBAL_FILES,
        GLOBAL_BYTES
    ));
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    connection.execute_batch("WITH RECURSIVE seq(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM seq WHERE x<1024) INSERT INTO _kenvoy_uploads(id,resource,field,owner,name,content_type,size,data,created_at,expires_at) SELECT printf('%064x',x),'customers','attachments',printf('%064x',x),'a.txt','text/plain',1,x'61',100,200 FROM seq")?;
    assert!(matches!(
        stage(&mut connection, &resource(), upload('a', 200), 100),
        Err(DatabaseError::UploadLimit)
    ));
    connection.execute("DELETE FROM _kenvoy_uploads WHERE id=printf('%064x',1)", [])?;
    stage(&mut connection, &resource(), upload('a', 200), 100)?;
    assert_eq!(count(&connection)?, GLOBAL_FILES);
    Ok(())
}

#[test]
fn migration_pruning_is_transactional_for_removed_or_retyped_fields_and_resources() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    let resource = resource();
    let file = stage(&mut connection, &resource, upload('a', 200), 100)?;
    let target = migration::SchemaModel::default();
    {
        let transaction = connection.transaction()?;
        cleanup_removed_uploads(&transaction, &target)?;
        assert_eq!(count(&transaction)?, 0);
        assert!(
            transaction
                .execute("INSERT INTO missing_upload_test_table VALUES(1)", [])
                .is_err()
        );
    }
    assert_eq!(count(&connection)?, 1);
    assert!(content(&connection, &file.id, 100)?.is_some());
    let mut scalar = resource.clone();
    scalar.fields[0].kind = FieldKind::Text;
    scalar.fields[0].upload = None;
    let target = migration::schema_model(&application(scalar))?;
    let transaction = connection.transaction()?;
    cleanup_removed_uploads(&transaction, &target)?;
    transaction.commit()?;
    assert_eq!(count(&connection)?, 0);
    stage(&mut connection, &resource, upload('a', 200), 100)?;
    let mut removed_field = migration::schema_model(&application(resource.clone()))?;
    removed_field
        .tables
        .get_mut("customers")
        .ok_or("缺少客户表")?
        .columns
        .clear();
    let transaction = connection.transaction()?;
    cleanup_removed_uploads(&transaction, &removed_field)?;
    transaction.commit()?;
    assert_eq!(count(&connection)?, 0);
    stage(&mut connection, &resource, upload('a', 200), 100)?;
    let transaction = connection.transaction()?;
    cleanup_removed_uploads(&transaction, &migration::SchemaModel::default())?;
    transaction.commit()?;
    assert_eq!(count(&connection)?, 0);
    Ok(())
}

#[test]
fn registered_file_models_reject_completely_missing_internal_storage() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    let app = application(resource());
    migration::apply_connection(&mut connection, &app, None, None)?;
    assert!(validate_schema(&connection)?);
    connection.execute_batch("DROP TABLE _kenvoy_uploads; DROP TABLE _kenvoy_upload_schema;")?;
    assert!(matches!(
        migration::apply_connection(&mut connection, &app, None, None),
        Err(migration::MigrationError::Drift(_))
    ));
    Ok(())
}

#[test]
fn session_byte_limit_uses_persisted_blob_sizes_instead_of_file_count() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    initialize(&mut connection, 100)?;
    let resource = resource();
    for _ in 0..4 {
        let mut input = upload('a', 200);
        input.data = vec![b'x'; MAX_UPLOAD_SIZE as usize];
        stage(&mut connection, &resource, input, 100)?;
    }
    assert_eq!(count(&connection)?, 4);
    assert!(matches!(
        stage(&mut connection, &resource, upload('a', 200), 100),
        Err(DatabaseError::UploadLimit)
    ));
    stage(&mut connection, &resource, upload('b', 200), 100)?;
    assert_eq!(count(&connection)?, 5);
    Ok(())
}
