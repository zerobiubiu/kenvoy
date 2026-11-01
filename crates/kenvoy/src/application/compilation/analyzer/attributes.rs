use std::collections::BTreeSet;

use serde_json::Value;

use crate::domain::{
    model::Role,
    source::{DslDiagnostic, DslDiagnosticCategory, SourceLocation},
};

use super::super::static_eval::{StaticElement, StaticNode, StaticValue};
use super::Result;

pub(super) fn error(
    element: &StaticElement,
    code: &str,
    message: impl Into<String>,
    suggestion: &str,
) -> DslDiagnostic {
    semantic_error(element.source.clone(), code, message, suggestion)
}

pub(super) fn semantic_error(
    source: SourceLocation,
    code: &str,
    message: impl Into<String>,
    suggestion: &str,
) -> DslDiagnostic {
    let mut diagnostic = DslDiagnostic::new(code, source, message, suggestion);
    diagnostic.category = DslDiagnosticCategory::Semantic;
    diagnostic
}

pub(super) fn elements(nodes: &[StaticNode]) -> Result<Vec<&StaticElement>> {
    let mut result = vec![];
    for node in nodes {
        match node {
            StaticNode::Element(element) => result.push(element),
            StaticNode::Text { value, source } if !value.trim().is_empty() => {
                return Err(semantic_error(
                    source.clone(),
                    "KV2001",
                    "此声明位置不允许文本内容",
                    "将展示文字放在页面或自定义组件中",
                ));
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn attrs(element: &StaticElement, allowed: &[&str]) -> Result<()> {
    if let Some(name) = element
        .attributes
        .keys()
        .find(|name| !allowed.contains(&name.as_str()))
    {
        return Err(error(
            element,
            "KV2002",
            format!("标签 {} 不支持属性 {name}", element.name),
            "检查标签属性规范和拼写",
        ));
    }
    Ok(())
}

pub(super) fn json<'a>(element: &'a StaticElement, name: &str) -> Result<Option<&'a Value>> {
    match element.attributes.get(name) {
        None => Ok(None),
        Some(StaticValue::Json(value)) => Ok(Some(value)),
        Some(_) => Err(error(
            element,
            "KV2003",
            format!("属性 {name} 必须是可静态确定的值"),
            "使用字面量或可静态确定的 const，不能执行函数生成 Schema",
        )),
    }
}

pub(super) fn string(element: &StaticElement, name: &str) -> Result<Option<String>> {
    json(element, name)?
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                error(
                    element,
                    "KV2004",
                    format!("属性 {name} 必须是字符串"),
                    "使用字符串字面量",
                )
            })
        })
        .transpose()
}

pub(super) fn required_string(element: &StaticElement, name: &str) -> Result<String> {
    string(element, name)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            error(
                element,
                "KV2004",
                format!("缺少非空字符串属性 {name}"),
                "为该属性提供非空字符串",
            )
        })
}

pub(super) fn boolean(element: &StaticElement, name: &str) -> Result<bool> {
    json(element, name)?
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                error(
                    element,
                    "KV2004",
                    format!("属性 {name} 必须是布尔值"),
                    "使用裸布尔属性或 {true}/{false}",
                )
            })
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}

pub(super) fn number(element: &StaticElement, name: &str) -> Result<Option<f64>> {
    json(element, name)?
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    error(
                        element,
                        "KV2004",
                        format!("属性 {name} 必须是有限数字"),
                        "使用有限数字字面量",
                    )
                })
        })
        .transpose()
}

pub(super) fn positive_integer(
    element: &StaticElement,
    name: &str,
    limit: u64,
) -> Result<Option<u64>> {
    number(element, name)?
        .map(|value| {
            if value < 1.0 || value > limit as f64 || value.fract() != 0.0 {
                Err(error(
                    element,
                    "KV2007",
                    format!("属性 {name} 必须为 1 到 {limit} 的整数"),
                    "使用支持范围内的正整数字面量",
                ))
            } else {
                Ok(value as u64)
            }
        })
        .transpose()
}

pub(super) fn list(element: &StaticElement, name: &str) -> Result<Option<Vec<String>>> {
    json(element, name)?
        .map(|value| {
            let items = value.as_array().ok_or_else(|| {
                error(
                    element,
                    "KV2004",
                    format!("属性 {name} 必须是字符串数组"),
                    "例如 fields={['name', 'phone']}",
                )
            })?;
            let mut output = Vec::new();
            let mut seen = BTreeSet::new();
            for item in items {
                let value = item.as_str().ok_or_else(|| {
                    error(
                        element,
                        "KV2004",
                        format!("属性 {name} 的每项必须是字符串"),
                        "检查数组中的非字符串值",
                    )
                })?;
                if !seen.insert(value) {
                    return Err(error(
                        element,
                        "KV2005",
                        format!("属性 {name} 包含重复字段 {value}"),
                        "每个字段只列出一次",
                    ));
                }
                output.push(value.to_owned());
            }
            Ok(output)
        })
        .transpose()
}

pub(super) fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.starts_with("sqlite_")
        && !name.starts_with("kenvoy_")
        && !matches!(
            name,
            "id" | "created_at" | "updated_at" | "actions" | "auth"
        )
}

pub(super) fn route(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains(['\\', '%', '?', '#', ':', '*', '{', '}', '\0'])
        && path.split('/').skip(1).all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
        })
        && !matches!(path.split('/').nth(1), Some("api" | "admin" | "_kenvoy"))
}

pub(super) fn role(element: &StaticElement, required: bool) -> Result<Role> {
    match string(element, "permission")?.as_deref() {
        Some("anonymous") => Ok(Role::Anonymous),
        Some("user") => Ok(Role::User),
        Some("admin") => Ok(Role::Admin),
        None if !required => Ok(Role::Anonymous),
        _ => Err(error(
            element,
            "KV2004",
            "permission 必须明确为 anonymous、user 或 admin",
            "服务端 Action 必须显式声明权限；匿名仅在确实允许公开调用时选择",
        )),
    }
}

pub(super) fn no_children(element: &StaticElement) -> Result<()> {
    if !elements(&element.children)?.is_empty() {
        return Err(error(
            element,
            "KV2001",
            format!("标签 {} 不允许子节点", element.name),
            "移除不受支持的子节点",
        ));
    }
    Ok(())
}

pub(super) fn intrinsic(element: &StaticElement) -> Result<()> {
    if element.module_ref.is_some() {
        return Err(error(
            element,
            "KV2001",
            format!("普通组件 {} 不能用于业务 Schema 或声明容器", element.name),
            "不要用同名组件遮蔽内置 DSL 标签；可复用 Schema 请使用 .diu.tsx",
        ));
    }
    Ok(())
}

pub(super) fn source_of(node: &StaticNode) -> SourceLocation {
    match node {
        StaticNode::Element(element) => element.source.clone(),
        StaticNode::Text { source, .. } => source.clone(),
    }
}

pub(super) fn start_of_file() -> kengine::SourceSpan {
    kengine::SourceSpan {
        start: 0,
        end: 0,
        start_position: kengine::Position { line: 1, column: 1 },
        end_position: kengine::Position { line: 1, column: 1 },
    }
}
