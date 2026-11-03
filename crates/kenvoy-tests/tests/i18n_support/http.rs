//! 描述只公开当前角色可见的原生词条；错误公开稳定机器键与规则参数。

use std::fs;

use kenvoy_tests::TestResult;
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::support::{Session, account, form, project, read_db};

#[tokio::test]
async fn application_metadata_filters_native_messages_for_anonymous_user_and_admin() -> TestResult {
    let fixture = project("i18n-projection")?;
    fs::write(
        fixture.project().join("src/app.tsx"),
        r#"export default <app name="词条权限">
      <resource name="customers">
        <text name="name" label="客户姓名" required maxLength={100}/>
        <text name="phone" label="联系电话"/>
        <number name="age" label="年龄" integer min={0}/>
      </resource>
      <page path="/customers" title="公开登记"><form resource="customers" fields={['name','age']}/><group label="公开分组">公开说明</group></page>
      <page path="/private" title="隐藏页面" permission="admin"><table resource="customers"/><group label="隐藏分组">隐藏说明</group></page>
      <navigation label="公共导航"><item path="/customers" label="公开入口"/><item path="/private" label="隐藏入口"/></navigation>
    </app>;"#,
    )?;
    let entries = [
        (
            "app.name",
            "词条权限",
            "Catalog permissions",
            "تطبيق العملاء",
        ),
        ("resources.customers.label", "客户", "Customers", "العملاء"),
        (
            "resources.customers.fields.name.label",
            "客户姓名",
            "Customer name",
            "اسم العميل",
        ),
        (
            "resources.customers.fields.phone.label",
            "私密联系电话",
            "Confidential phone label",
            "هاتف خاص",
        ),
        (
            "resources.customers.fields.age.label",
            "年龄",
            "Age",
            "العمر",
        ),
        (
            "resources.unknown.fields.secret.label",
            "未声明字段",
            "Undeclared field label",
            "حقل غير معلن",
        ),
        (
            "pages./customers.title",
            "公开登记",
            "Public registration",
            "تسجيل عام",
        ),
        (
            "pages./private.title",
            "隐藏页面",
            "Private review title",
            "مراجعة خاصة",
        ),
        (
            "pages./missing.title",
            "未声明页面",
            "Undeclared page title",
            "صفحة غير معلنة",
        ),
        ("text:公开分组", "公开分组", "Public section", "قسم عام"),
        ("text:公开说明", "公开说明", "Public help", "مساعدة عامة"),
        (
            "text:隐藏分组",
            "隐藏分组",
            "Private section label",
            "قسم خاص",
        ),
        (
            "text:隐藏说明",
            "隐藏说明",
            "Private explanation",
            "شرح خاص",
        ),
        ("text:公共导航", "公共导航", "Navigation", "التنقل"),
        ("text:公开入口", "公开入口", "Register", "التسجيل"),
        (
            "text:隐藏入口",
            "隐藏入口",
            "Private navigation label",
            "تنقل خاص",
        ),
        (
            "text:不存在的隐藏原文",
            "未声明原文",
            "Undeclared raw message",
            "نص غير معلن",
        ),
        (
            "custom.public",
            "客户端文案",
            "Public client message",
            "رسالة عامة",
        ),
        (
            "ui.project_hint",
            "界面提示",
            "Project UI message",
            "رسالة الواجهة",
        ),
        (
            "errors.project_rule",
            "规则提示",
            "Project rule message",
            "رسالة القاعدة",
        ),
    ];
    let mut translations = json!({"$schema":"./language.schema.json"});
    for (key, chinese, english, arabic) in entries {
        translations[key] = json!({"cn":chinese,"en-US":english,"ar":arabic});
    }
    fs::write(
        fixture.project().join("translations.json"),
        serde_json::to_vec(&translations)?,
    )?;
    fs::write(
        fixture.project().join("kenvoy.toml"),
        "[project]\nname='app'\nversion='0.1.0'\nmin_cli_version='0.4.0'\nformat_version='0.4.0'\n[server]\nport=0\n[languages]\nfallback='zh-CN'\n[languages.zh-CN]\nalias='cn'\ndisplay='中文界面'\n[languages.en-US]\nalias='en'\ndisplay='Custom English'\n[languages.ar]\ndisplay='العربية'\n",
    )?;
    account(&fixture, "init", "administrator")?;
    account(&fixture, "add-user", "reader")?;
    let mut server = fixture.start("run")?;
    let mut session = Session::new(server.origin()).await?;
    let anonymous = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(anonymous["format_version"], 3);
    assert_eq!(anonymous["role"], "anonymous");
    assert_eq!(
        anonymous["name"], "词条权限",
        "HTTP 默认文案不是根据 Accept-Language 改写的数据"
    );
    assert_eq!(anonymous["i18n"]["format_version"], 2);
    assert_eq!(anonymous["i18n"]["fallback"], "zh-CN");
    let languages = anonymous["i18n"]["languages"]
        .as_array()
        .ok_or("缺少语言列表")?;
    assert_eq!(languages.len(), 3);
    for option in [
        json!({"code":"zh-CN","alias":"cn","display":"中文界面"}),
        json!({"code":"en-US","alias":"en","display":"Custom English"}),
        json!({"code":"ar","display":"العربية"}),
    ] {
        assert!(
            languages.contains(&option),
            "必须保留配置语言信息：{option}"
        );
    }
    assert!(anonymous["i18n"]["messages"].get("cn").is_none());
    assert!(anonymous["i18n"]["messages"].get("en").is_none());
    assert_eq!(
        anonymous["i18n"]["messages"]["ar"]["app.name"],
        "تطبيق العملاء"
    );
    assert_eq!(
        anonymous["i18n"]["messages"]["ar"]["resources.customers.fields.name.label"],
        "اسم العميل"
    );
    assert!(
        anonymous["i18n"]["messages"]["ar"]
            .get("resources.customers.fields.phone.label")
            .is_none()
    );
    let public = &anonymous["i18n"]["messages"]["en-US"];
    for (key, value) in [
        ("app.name", "Catalog permissions"),
        ("resources.customers.label", "Customers"),
        ("resources.customers.fields.name.label", "Customer name"),
        ("pages./customers.title", "Public registration"),
        ("text:公开分组", "Public section"),
        ("text:公开说明", "Public help"),
        ("custom.public", "Public client message"),
        ("ui.project_hint", "Project UI message"),
        ("errors.project_rule", "Project rule message"),
    ] {
        assert_eq!(public[key], value, "公开词条 {key}");
    }
    let hidden = [
        "resources.customers.fields.phone.label",
        "resources.unknown.fields.secret.label",
        "pages./private.title",
        "pages./missing.title",
        "text:隐藏分组",
        "text:隐藏说明",
        "text:隐藏入口",
        "text:不存在的隐藏原文",
    ];
    for code in ["zh-CN", "en-US", "ar"] {
        for key in hidden {
            assert!(
                anonymous["i18n"]["messages"][code].get(key).is_none(),
                "匿名投影泄露 {code}/{key}"
            );
        }
    }
    assert!(
        anonymous["pages"]
            .as_array()
            .ok_or("缺少页面")?
            .iter()
            .all(|page| page["path"] != "/private")
    );
    assert!(
        anonymous["resources"][0]["fields"]
            .as_array()
            .ok_or("缺少字段")?
            .iter()
            .all(|field| field["name"] != "phone")
    );
    session.login("reader").await?;
    let reader = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(reader["role"], "user");
    for code in ["zh-CN", "en-US", "ar"] {
        for key in hidden {
            assert!(
                reader["i18n"]["messages"][code].get(key).is_none(),
                "普通用户投影泄露 {code}/{key}"
            );
        }
    }
    session.logout().await?;
    session.login("administrator").await?;
    let admin = session.get("/_kenvoy/application", StatusCode::OK).await?;
    assert_eq!(admin["role"], "admin");
    let private = &admin["i18n"]["messages"]["en-US"];
    assert_eq!(
        private["resources.customers.fields.phone.label"],
        "Confidential phone label"
    );
    assert_eq!(private["pages./private.title"], "Private review title");
    assert_eq!(private["text:隐藏分组"], "Private section label");
    assert_eq!(private["text:隐藏说明"], "Private explanation");
    assert_eq!(private["text:隐藏入口"], "Private navigation label");
    assert!(private.get("pages./missing.title").is_none());
    for path in [
        "/translations.json",
        "/language.schema.json",
        "/i18n.json",
        "/_kenvoy/i18n",
    ] {
        let response = session
            .client
            .get(format!("{}{path}", server.origin()))
            .send()
            .await?;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "词典不能从静态根或全量端点公开：{path}"
        );
    }
    server.stop()?;
    assert_eq!(
        read_db(&fixture)?.query_row("SELECT COUNT(*) FROM customers", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

fn error(value: &Value, code: &str) {
    assert_eq!(value["error"]["code"], code);
    assert_eq!(value["error"]["message_key"], code);
    assert_eq!(value["error"]["params"], json!({}));
}
fn field(value: &Value, name: &str, code: &str, key: &str, params: Value) -> TestResult {
    let field = value["error"]["fields"]
        .as_array()
        .ok_or("缺少字段错误")?
        .iter()
        .find(|field| field["field"] == name)
        .ok_or("缺少指定字段错误")?;
    assert_eq!(field["code"], code);
    assert_eq!(field["message_key"], key);
    assert_eq!(field["params"], params);
    Ok(())
}

#[tokio::test]
async fn http_and_field_errors_expose_machine_keys_and_exact_rule_parameters() -> TestResult {
    let fixture = project("i18n-errors")?;
    account(&fixture, "init", "administrator")?;
    account(&fixture, "add-user", "reader")?;
    let mut server = fixture.start("run")?;
    let mut session = Session::new(server.origin()).await?;
    let model = session.get("/_kenvoy/application", StatusCode::OK).await?;
    let form = form(&model)?;
    let invalid = session
        .post(
            "/api/customers",
            json!({"name":"  ","age":-1}),
            Some(&form),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await?;
    error(&invalid, "KV5022");
    field(
        &invalid,
        "name",
        "KV4001",
        "validation.required.text",
        json!({}),
    )?;
    field(
        &invalid,
        "age",
        "KV4005",
        "validation.number.min",
        json!({"min":0.0}),
    )?;
    let invalid = session
        .post(
            "/api/customers",
            json!({"name":"😀".repeat(101),"age":1.5}),
            Some(&form),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await?;
    field(
        &invalid,
        "name",
        "KV4003",
        "validation.text.max_length",
        json!({"max":100}),
    )?;
    field(
        &invalid,
        "age",
        "KV4004",
        "validation.number.integer",
        json!({"min":-9_007_199_254_740_991_i64,"max":9_007_199_254_740_991_i64}),
    )?;
    let invalid = session
        .post(
            "/api/customers",
            json!({"name":null}),
            Some(&form),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await?;
    field(&invalid, "name", "KV4001", "validation.required", json!({}))?;
    error(
        &session
            .get("/api/customers", StatusCode::UNAUTHORIZED)
            .await?,
        "KV5001",
    );
    error(
        &session.get("/api/missing", StatusCode::NOT_FOUND).await?,
        "KV5004",
    );
    let rejected = session
        .client
        .post(format!("{}/api/customers", server.origin()))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&json!({"name":"没有同源校验"}))?)
        .send()
        .await?;
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    let rejected: Value = serde_json::from_slice(&rejected.bytes().await?)?;
    error(&rejected, "KV5013");
    session.login("reader").await?;
    error(
        &session.get("/api/customers", StatusCode::FORBIDDEN).await?,
        "KV5003",
    );
    session.logout().await?;
    let created = session
        .post(
            "/api/customers",
            json!({"name":"保留 {name} / Customer","phone":"13800000000","age":29}),
            Some(&form),
            StatusCode::CREATED,
        )
        .await?;
    let id = created["data"]["id"].as_i64().ok_or("记录缺少 ID")?;
    assert_eq!(created["data"]["name"], "保留 {name} / Customer");
    session.login("administrator").await?;
    let stored = session
        .get(&format!("/api/customers/{id}"), StatusCode::OK)
        .await?;
    assert_eq!(stored["data"]["name"], "保留 {name} / Customer");
    server.stop()?;
    let database = read_db(&fixture)?;
    assert_eq!(
        database.query_row("SELECT COUNT(*) FROM customers", [], |row| row
            .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        database.query_row("SELECT name FROM customers WHERE id=?1", [id], |row| row
            .get::<_, String>(
            0
        ))?,
        "保留 {name} / Customer"
    );
    Ok(())
}
