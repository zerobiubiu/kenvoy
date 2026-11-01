use std::path::Path;

use super::error::{CliError as Error, Result};
use crate::{
    application::identity::UserView,
    domain::model::{ApplicationIR, FieldKind, PermissionTarget},
};

pub(crate) fn created(path: &Path) {
    println!(
        "已创建项目：{}\n下一步：进入项目目录，执行 kenvoy dev",
        path.display()
    );
}

pub(crate) fn build_completed(name: &str, build_id: &str, manifest: &Path) {
    println!(
        "构建完成：{}\n版本：{}\n清单：{}",
        name,
        build_id,
        manifest.display()
    );
}

pub(crate) fn print_json(
    value: &impl serde::Serialize,
) -> std::result::Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(crate) fn record_validated(
    value: &impl serde::Serialize,
) -> std::result::Result<(), serde_json::Error> {
    println!(
        "记录验证通过（未写入数据库）：\n{}",
        serde_json::to_string_pretty(value)?
    );
    Ok(())
}

pub(crate) fn check_succeeded(application: &ApplicationIR, database: &Path, out_dir: &Path) {
    println!(
        "检查通过：{}，{} 个资源，{} 个页面，{} 个动作\n数据库路径：{}\n构建输出：{}",
        application.name,
        application.resources.len(),
        application.pages.len(),
        application.actions.len(),
        database.display(),
        out_dir.display()
    );
}

pub(crate) fn account_created(user: &UserView) {
    println!("已创建账号：{}，角色：{:?}", user.username, user.role);
}

pub(crate) fn server_ready(origin: &str) {
    println!("Kenvoy 应用：{origin}\n管理登录：{origin}/admin/login");
}

pub(crate) fn inspect_resource(application: &ApplicationIR, name: &str) -> Result<()> {
    let resource = application
        .resource(name)
        .ok_or_else(|| Error::InvalidConfig(format!("资源不存在：{name}")))?;
    println!("资源：{}\n\n字段：", resource.name);
    for field in &resource.fields {
        let storage = match field.kind {
            FieldKind::Number if !field.integer => "REAL",
            FieldKind::Number | FieldKind::Boolean => "INTEGER",
            _ => "TEXT",
        };
        println!(
            "  {}\t{}\t{}",
            field.name,
            storage,
            if field.required { "必填" } else { "可选" }
        );
    }
    println!(
        "\n数据库表：{name}\n\n接口注册计划：\n  GET/POST /api/{name}\n  GET/PATCH/DELETE /api/{name}/:id\n\n后台：/admin/{name}\n\n授权策略（本命令没有登录身份，不代表当前用户可调用）："
    );
    for permission in &application.permissions {
        if permission.target == PermissionTarget::Resource(name.to_owned()) {
            println!(
                "  {:?} {:?}，字段：{}",
                permission.role,
                permission.operation,
                permission.fields.join(", ")
            );
        }
    }
    Ok(())
}
