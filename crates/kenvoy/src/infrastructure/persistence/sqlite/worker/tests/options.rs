use super::*;

#[tokio::test]
async fn linked_options_compare_canonical_numbers_and_datetimes() -> TestResult {
    let fixture = Fixture::new()?;
    let app = fixture.application(r#"<number name='count' integer/><number name='measure'/><datetime name='occurred'/><select name='choice'><option value='yes' when={{count:1.0,measure:1,occurred:'2026-11-01T08:00:00+08:00'}}/></select>"#)?;
    let database = Database::open(fixture.database.clone(), app).await?;
    let created = database
        .create(
            "customers",
            json!({"count":1,"measure":1.0,"occurred":"2026-11-01T00:00:00Z","choice":"yes"}),
            None,
        )
        .await?;
    assert_eq!(created["occurred"], "2026-11-01T00:00:00.000Z");
    let id = created["id"].as_i64().ok_or("缺少 id")?;
    assert!(matches!(
        database
            .update("customers", id, json!({"count":2}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database.get("customers", id).await?.ok_or("缺少原记录")?,
        created
    );
    database.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn linked_and_resource_options_validate_inside_write_transactions() -> TestResult {
    let fixture = Fixture::new()?;
    let entry = fixture.root.join("src/app.tsx");
    fs::write(
        &entry,
        r#"export default <app name='联动'><resource name='cities'><text name='code' unique required/><text name='name' required/><text name='province' required/><text name='secret'/></resource><resource name='customers'><select name='province' required><option value='zj'/><option value='js'/></select><select name='city' required optionsSource={{resource:'cities',value:'code',label:'name',filters:{province:'province'},permission:'anonymous'}}/><select name='district'><option value='xihu' when={{province:'zj'}}/><option value='gulou' when={{province:'js'}}/></select></resource></app>;"#,
    )?;
    let program = kengine::Engine::new().analyze_project(
        &fixture.root,
        kengine::AnalyzeOptions {
            entry_points: vec![entry.clone()],
            ..Default::default()
        },
    )?;
    let result = crate::application::compilation::analyze(
        &program,
        &entry.canonicalize()?,
        "选项",
        &crate::infrastructure::javascript::SwcClientCodeTransformer,
    );
    let app = result
        .application
        .ok_or_else(|| format!("{:?}", result.diagnostics))?;
    let database = Database::open(fixture.database.clone(), app.clone()).await?;
    let mut labels_only = app.clone();
    let linked = labels_only
        .resources
        .iter_mut()
        .find(|resource| resource.name == "customers")
        .ok_or("缺少客户资源")?;
    linked
        .fields
        .iter_mut()
        .find(|field| field.name == "district")
        .ok_or("缺少区字段")?
        .options[0]
        .label = "西湖区".into();
    linked
        .fields
        .iter_mut()
        .find(|field| field.name == "district")
        .ok_or("缺少区字段")?
        .options[0]
        .when
        .insert("province".into(), json!("js"));
    assert_eq!(
        migration::schema_model(&labels_only)?,
        migration::schema_model(&app)?
    );
    assert!(!migration::preview(&fixture.database, &labels_only)?.destructive);
    let mut fixed = app.clone();
    let remote = fixed
        .resources
        .iter_mut()
        .find(|resource| resource.name == "customers")
        .and_then(|resource| {
            resource
                .fields
                .iter_mut()
                .find(|field| field.name == "city")
        })
        .ok_or("缺少城市字段")?;
    remote.options_source = None;
    remote.options.push(crate::domain::model::SelectOption {
        value: "hz".into(),
        label: "杭州".into(),
        when: Default::default(),
    });
    assert!(migration::preview(&fixture.database, &fixed)?.destructive);
    let hz = database
        .create(
            "cities",
            json!({"code":"hz","name":"杭州","province":"zj","secret":"私有"}),
            None,
        )
        .await?;
    database
        .create(
            "cities",
            json!({"code":"nb","name":"宁波","province":"zj"}),
            None,
        )
        .await?;
    database
        .create(
            "cities",
            json!({"code":"nj","name":"南京","province":"js"}),
            None,
        )
        .await?;
    let request: OptionsQuery = serde_json::from_value(
        json!({"dependencies":{"province":"zj"},"page_size":1,"selected":"hz"}),
    )?;
    let result = database.options("customers", "city", request).await?;
    assert_eq!(result.total, 2);
    assert_eq!(result.data.len(), 1);
    assert_eq!(result.selected.ok_or("缺少选中项")?.label, "杭州");
    let result = database
        .options(
            "customers",
            "city",
            serde_json::from_value(json!({"dependencies":{"province":"js"},"selected":"hz"}))?,
        )
        .await?;
    assert!(result.selected.is_none());
    assert_eq!(result.data[0].value, "nj");
    for invalid in [
        json!({"dependencies":{"secret":"私有"}}),
        json!({"dependencies":{},"page_size":101}),
        json!({"dependencies":{},"page":0}),
    ] {
        assert!(
            database
                .options("customers", "city", serde_json::from_value(invalid)?)
                .await
                .is_err()
        );
    }
    let empty = database
        .options("customers", "city", Default::default())
        .await?;
    assert_eq!(empty.total, 0);
    let created = database
        .create(
            "customers",
            json!({"province":"zj","city":"hz","district":"xihu"}),
            None,
        )
        .await?;
    let id = created["id"].as_i64().ok_or("缺少 id")?;
    for invalid in [
        json!({"province":"js","city":"hz"}),
        json!({"province":"zj","city":"missing"}),
        json!({"province":"zj","city":"hz","district":"gulou"}),
    ] {
        assert!(matches!(
            database.create("customers", invalid, None).await,
            Err(DatabaseError::Validation(_))
        ));
    }
    assert!(matches!(
        database
            .update("customers", id, json!({"province":"js"}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    assert_eq!(
        database.get("customers", id).await?.ok_or("记录丢失")?,
        created
    );
    database
        .update(
            "customers",
            id,
            json!({"province":"js","city":"nj","district":"gulou"}),
            None,
        )
        .await?;
    database
        .delete("cities", hz["id"].as_i64().ok_or("缺少源 id")?)
        .await?;
    assert!(matches!(
        database
            .create("customers", json!({"province":"zj","city":"hz"}), None)
            .await,
        Err(DatabaseError::Validation(_))
    ));
    database.shutdown().await?;
    let reopened = Database::open(fixture.database.clone(), app).await?;
    assert_eq!(
        reopened.get("customers", id).await?.ok_or("持久化失败")?["city"],
        "nj"
    );
    reopened.shutdown().await?;
    Ok(())
}
