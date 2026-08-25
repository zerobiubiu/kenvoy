//! 用户 / 角色 / 通讯录 / 部门 CLI 管理命令（需要 `db` feature）。

#![cfg(feature = "db")]

use std::path::Path;

use crate::db::UserStore;
use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::config;

/// 连接项目数据库（读 `kenvoy.toml [database].url`）。
async fn connect(dir: &Path) -> Result<PgPool> {
    let cfg = config::read_config(dir);
    let url = cfg
        .database
        .url
        .as_deref()
        .context("kenvoy.toml 未配置 [database].url")?;
    let pool = crate::db::connect(url).await?;
    crate::db::migrate(&pool).await?;
    Ok(pool)
}

async fn find_role(store: &UserStore<'_>, name: &str) -> Result<crate::db::users::Role> {
    store
        .list_roles()
        .await?
        .into_iter()
        .find(|r| r.name == name)
        .with_context(|| format!("角色不存在：{name}"))
}

async fn find_user(store: &UserStore<'_>, username: &str) -> Result<i64> {
    store
        .list_users()
        .await?
        .into_iter()
        .find(|u| u.user.username == username)
        .map(|u| u.user.id)
        .with_context(|| format!("用户不存在：{username}"))
}

async fn find_dept(store: &UserStore<'_>, name: &str) -> Result<i64> {
    store
        .list_departments()
        .await?
        .into_iter()
        .find(|d| d.name == name)
        .map(|d| d.id)
        .with_context(|| format!("部门不存在：{name}"))
}

/// 创建用户（可选分配角色）。
/// 添加用户（CLI）。
///
/// # Errors
/// 数据库错误、用户名冲突、密码强度不符时返回。
pub async fn user_add(
    dir: &Path,
    username: &str,
    password: &str,
    display_name: &str,
    role: Option<&str>,
) -> Result<()> {
    // 密码强度校验（`kenvoy.toml [auth].password`，与 HTTP 注册一致）
    let cfg = config::read_config(dir);
    let policy = crate::config::password_policy(&cfg.auth.password);
    let errors = policy.validate(password, username);
    if !errors.is_empty() {
        anyhow::bail!("密码不满足强度要求：{}", errors.join("；"));
    }
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    let uid = store.create_user(username, password, display_name).await?;
    if let Some(role_name) = role {
        let role = find_role(&store, role_name).await?;
        store.assign_role(uid, role.id).await?;
    }
    println!("已创建用户：{username}（id={uid}）");
    Ok(())
}

/// 列出用户（含角色与账号等级）。
/// 列出用户（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn user_list(dir: &Path) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    for u in store.list_users().await? {
        println!(
            "{:<4} {:<20} {:<12} 等级={:<4} 角色=[{}]",
            u.user.id,
            u.user.username,
            u.user.display_name,
            u.level,
            u.roles.join(", ")
        );
    }
    Ok(())
}

/// 创建角色（账号等级）。
/// 创建角色（CLI）。
///
/// # Errors
/// 数据库错误、角色名重复时返回。
pub async fn role_add(dir: &Path, name: &str, level: i32, description: Option<&str>) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    let id = store.create_role(name, level, description).await?;
    println!("已创建角色：{name}（id={id}，等级={level}）");
    Ok(())
}

/// 列出角色。
/// 列出角色（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn role_list(dir: &Path) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    for r in store.list_roles().await? {
        println!(
            "{:<4} {:<20} 等级={:<4} {}",
            r.id,
            r.name,
            r.level,
            r.description.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

/// 新增通讯录人员（可无账号；`--user` 关联账号）。
/// 新增通讯录人员（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn contact_add(
    dir: &Path,
    name: &str,
    department: Option<&str>,
    user: Option<&str>,
    email: Option<&str>,
    phone: Option<&str>,
    title: Option<&str>,
) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    let dept_id = match department {
        Some(d) => Some(find_dept(&store, d).await?),
        None => None,
    };
    let user_id = match user {
        Some(u) => Some(find_user(&store, u).await?),
        None => None,
    };
    let id = store
        .add_contact(name, dept_id, user_id, email, phone, title)
        .await?;
    println!("已新增通讯录：{name}（id={id}{}）", {
        if user_id.is_some() {
            "，已关联账号"
        } else {
            "，未关联账号"
        }
    });
    Ok(())
}

/// 列出通讯录。
/// 列出通讯录（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn contact_list(dir: &Path, keyword: Option<&str>) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    for c in store.list_contacts(keyword).await? {
        println!(
            "{:<4} {:<12} 部门id={:<4} 账号id={:<4} {} {}",
            c.id,
            c.name,
            c.department_id
                .map_or_else(|| "-".into(), |v| v.to_string()),
            c.user_id.map_or_else(|| "-".into(), |v| v.to_string()),
            c.title.as_deref().unwrap_or(""),
            c.status
        );
    }
    Ok(())
}

/// 创建部门。
/// 创建部门（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn dept_add(dir: &Path, name: &str, parent: Option<&str>) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    let parent_id = match parent {
        Some(p) => Some(find_dept(&store, p).await?),
        None => None,
    };
    let id = store.create_department(name, parent_id).await?;
    println!("已创建部门：{name}（id={id}）");
    Ok(())
}

/// 列出部门。
/// 列出部门（CLI）。
///
/// # Errors
/// 数据库错误时返回。
pub async fn dept_list(dir: &Path) -> Result<()> {
    let pool = connect(dir).await?;
    let store = UserStore::new(&pool);
    for d in store.list_departments().await? {
        println!(
            "{:<4} {:<16} 上级id={}",
            d.id,
            d.name,
            d.parent_id.map_or_else(|| "-".into(), |v| v.to_string())
        );
    }
    Ok(())
}
