//! DSL 解析：XML 业务语言 → [`crate::ir::ProjectIr`]。
//!
//! kengine 为无 I/O 纯解析引擎：输入「文件路径 + 内容」列表，输出 `IR`；
//! 文件系统扫描由 CLI（`kenvoy`）负责。

mod doc;
mod node;

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ir::{IR_VERSION, Layout, Model, Node, Page, Permissions, ProjectIr, ProjectMeta};

/// 一个待解析的源文件（无 I/O：由调用方读取内容）。
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// 项目内相对路径（如 `apps/leave/apply/page.kenvoy.xml`），分隔符统一为 `/`。
    pub path: String,
    /// 文件内容。
    pub content: String,
}

/// 解析错误。
#[derive(Debug, Error)]
pub enum ParseError {
    /// XML 解析失败。
    #[error("{path}: XML 解析失败：{source}")]
    Xml {
        /// 文件路径。
        path: String,
        /// 底层 XML 错误。
        source: roxmltree::Error,
    },
    /// 元素缺少必需属性。
    #[error("{path}: <{element}> 缺少必需属性 `{attr}`")]
    MissingAttribute {
        /// 文件路径。
        path: String,
        /// 元素名。
        element: String,
        /// 属性名。
        attr: String,
    },
    /// 缺少必需子元素。
    #[error("{path}: 缺少必需元素 <{element}>")]
    MissingElement {
        /// 文件路径。
        path: String,
        /// 元素名。
        element: String,
    },
    /// 无效字段类型。
    #[error("{path}: 无效字段类型 `{value}`")]
    InvalidFieldType {
        /// 文件路径。
        path: String,
        /// 非法值。
        value: String,
    },
    /// 属性值无效。
    #[error("{path}: 属性 `{attr}` 的值 `{value}` 无效")]
    InvalidAttribute {
        /// 文件路径。
        path: String,
        /// 属性名。
        attr: String,
        /// 非法值。
        value: String,
    },
    /// 根布局缺失。
    #[error("根布局缺失：apps/ 下必须存在 layout.kenvoy.xml")]
    RootLayoutMissing,
    /// 引用了未定义的模型。
    #[error("{from}: 引用的模型 `{model}` 未定义")]
    ModelNotFound {
        /// 引用来源（页面路径）。
        from: String,
        /// 模型名。
        model: String,
    },
    /// 模型中不存在该字段。
    #[error("{from}: 模型 `{model}` 中不存在字段 `{field}`")]
    FieldNotFound {
        /// 引用来源（页面路径）。
        from: String,
        /// 模型名。
        model: String,
        /// 字段名。
        field: String,
    },
    /// 模型名重复。
    #[error("模型名重复：`{name}`")]
    DuplicateModel {
        /// 重复的模型名。
        name: String,
    },
    /// 布局路径重复。
    #[error("布局路径重复：`{path}`")]
    DuplicateLayout {
        /// 重复的布局路径。
        path: String,
    },
    /// 页面路径重复。
    #[error("页面路径重复：`{path}`")]
    DuplicatePage {
        /// 重复的页面路径。
        path: String,
    },
}

impl ParseError {
    /// 构造 [`ParseError::MissingAttribute`]。
    pub(crate) fn missing_attribute(path: &str, element: &str, attr: &str) -> Self {
        Self::MissingAttribute {
            path: path.to_string(),
            element: element.to_string(),
            attr: attr.to_string(),
        }
    }

    /// 构造 [`ParseError::InvalidAttribute`]。
    pub(crate) fn invalid_attribute(path: &str, attr: &str, value: &str) -> Self {
        Self::InvalidAttribute {
            path: path.to_string(),
            attr: attr.to_string(),
            value: value.to_string(),
        }
    }
}

/// 文件角色（按文件名识别）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// 数据模型文件（`models.kenvoy.xml`）。
    Models,
    /// 布局文件（`layout.kenvoy.xml`）。
    Layout,
    /// 页面文件（`page.kenvoy.xml`）。
    Page,
}

/// 解析一组源文件，构建完整 `IR`。
///
/// 约定：源文件必须位于 `apps/` 目录下；`apps/layout.kenvoy.xml` 为根布局（必须存在）；
/// layout 逐级匹配，页面路径由目录结构推导。
///
/// # Errors
/// 返回 [`ParseError`]：XML 语法错误、结构校验失败、引用错误等。
pub fn build_project(files: &[SourceFile], project_name: &str) -> Result<ProjectIr, ParseError> {
    let mut models: BTreeMap<String, Model> = BTreeMap::new();
    let mut layouts: BTreeMap<String, Layout> = BTreeMap::new();
    let mut pages: Vec<Page> = Vec::new();

    for file in files {
        let Some((role, dir)) = classify(&file.path) else {
            continue;
        };
        let doc = roxmltree::Document::parse(&file.content).map_err(|source| ParseError::Xml {
            path: file.path.clone(),
            source,
        })?;
        let root = doc.root_element();
        match role {
            Role::Models => {
                for model in doc::parse_models(&file.path, root)? {
                    if models.contains_key(&model.name) {
                        return Err(ParseError::DuplicateModel { name: model.name });
                    }
                    models.insert(model.name.clone(), model);
                }
            }
            Role::Layout => {
                let layout = doc::parse_layout(&dir, root)?;
                if layouts.contains_key(&layout.path) {
                    return Err(ParseError::DuplicateLayout { path: layout.path });
                }
                layouts.insert(layout.path.clone(), layout);
            }
            Role::Page => {
                let page = doc::parse_page(&dir, root)?;
                if pages.iter().any(|p| p.path == page.path) {
                    return Err(ParseError::DuplicatePage { path: page.path });
                }
                pages.push(page);
            }
        }
    }

    if !layouts.contains_key("/") {
        return Err(ParseError::RootLayoutMissing);
    }

    for page in &mut pages {
        page.layout_chain = resolve_layout_chain(&page.path, &layouts);
    }

    validate_models(&pages, &models)?;

    Ok(ProjectIr {
        version: IR_VERSION.to_string(),
        project: ProjectMeta {
            name: project_name.to_string(),
            description: None,
        },
        models,
        layouts,
        pages,
        flows: Vec::new(),
        permissions: Permissions::default(),
    })
}

/// 依据文件路径识别角色与「apps 内目录路径」。
///
/// 例如 `apps/leave/apply/page.kenvoy.xml` → (`Page`, `/leave/apply`)。
fn classify(path: &str) -> Option<(Role, String)> {
    let norm = path.replace('\\', "/");
    let (dir, file) = norm.rsplit_once('/')?;
    let role = match file {
        "layout.kenvoy.xml" => Role::Layout,
        "page.kenvoy.xml" => Role::Page,
        "models.kenvoy.xml" => Role::Models,
        _ => return None,
    };
    let dir = dir.strip_prefix("apps")?;
    let dir_path = if dir.is_empty() {
        "/".to_string()
    } else {
        dir.to_string()
    };
    Some((role, dir_path))
}

/// 解析页面布局链（外层在前）：根 → 各级目录 → 页面目录自身（若存在 layout）。
fn resolve_layout_chain(page_path: &str, layouts: &BTreeMap<String, Layout>) -> Vec<String> {
    let mut chain = Vec::new();
    if layouts.contains_key("/") {
        chain.push("/".to_string());
    }
    let mut prefix = String::new();
    for seg in page_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
    {
        prefix.push('/');
        prefix.push_str(seg);
        if layouts.contains_key(&prefix) {
            chain.push(prefix.clone());
        }
    }
    chain
}

/// 校验页面中的模型与字段引用。
fn validate_models(pages: &[Page], models: &BTreeMap<String, Model>) -> Result<(), ParseError> {
    for page in pages {
        validate_node(&page.content, models, &page.path, None)?;
    }
    Ok(())
}

/// 递归校验节点树：`Form` / `Table` 引用的模型必须存在，`Field` 字段必须属于当前模型。
fn validate_node(
    node: &Node,
    models: &BTreeMap<String, Model>,
    from: &str,
    current_model: Option<&str>,
) -> Result<(), ParseError> {
    match node {
        Node::Form { props, children } => {
            if !models.contains_key(&props.model) {
                return Err(ParseError::ModelNotFound {
                    from: from.to_string(),
                    model: props.model.clone(),
                });
            }
            for child in children {
                validate_node(child, models, from, Some(&props.model))?;
            }
        }
        Node::Field { props } => {
            if let Some(model_name) = current_model {
                let field_exists = models
                    .get(model_name)
                    .is_some_and(|m| m.fields.iter().any(|f| f.name == props.name));
                if !field_exists {
                    return Err(ParseError::FieldNotFound {
                        from: from.to_string(),
                        model: model_name.to_string(),
                        field: props.name.clone(),
                    });
                }
            }
        }
        Node::Table { props, children } => {
            if !props.model.is_empty() && !models.contains_key(&props.model) {
                return Err(ParseError::ModelNotFound {
                    from: from.to_string(),
                    model: props.model.clone(),
                });
            }
            for child in children {
                validate_node(child, models, from, None)?;
            }
        }
        Node::Row { children, .. }
        | Node::Col { children, .. }
        | Node::Section { children, .. }
        | Node::Custom { children, .. } => {
            for child in children {
                validate_node(child, models, from, current_model)?;
            }
        }
        Node::Slot | Node::Column { .. } | Node::StatCard { .. } | Node::Text { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_layout(path: &str) -> Layout {
        Layout {
            path: path.to_string(),
            title: None,
            root: Node::Slot,
        }
    }

    #[test]
    fn classify_paths() {
        assert_eq!(
            classify("apps/layout.kenvoy.xml"),
            Some((Role::Layout, "/".to_string()))
        );
        assert_eq!(
            classify("apps/leave/layout.kenvoy.xml"),
            Some((Role::Layout, "/leave".to_string()))
        );
        assert_eq!(
            classify("apps/leave/apply/page.kenvoy.xml"),
            Some((Role::Page, "/leave/apply".to_string()))
        );
        assert_eq!(
            classify("apps/leave/models.kenvoy.xml"),
            Some((Role::Models, "/leave".to_string()))
        );
        assert_eq!(classify("README.md"), None);
        assert_eq!(classify("apps/leave/apply/flow.kenvoy.xml"), None);
    }

    #[test]
    fn layout_chain_resolution() {
        let layouts = BTreeMap::from([
            ("/".to_string(), dummy_layout("/")),
            ("/leave".to_string(), dummy_layout("/leave")),
        ]);
        assert_eq!(
            resolve_layout_chain("/leave/apply", &layouts),
            vec!["/".to_string(), "/leave".to_string()]
        );
        // 页面目录自身有 layout 时也纳入链尾
        let layouts = BTreeMap::from([
            ("/".to_string(), dummy_layout("/")),
            ("/leave".to_string(), dummy_layout("/leave")),
            ("/leave/apply".to_string(), dummy_layout("/leave/apply")),
        ]);
        assert_eq!(
            resolve_layout_chain("/leave/apply", &layouts),
            vec![
                "/".to_string(),
                "/leave".to_string(),
                "/leave/apply".to_string()
            ]
        );
    }

    #[test]
    fn missing_root_layout_errors() {
        let files = vec![SourceFile {
            path: "apps/leave/apply/page.kenvoy.xml".into(),
            content: "<Page><Content><Text value=\"x\"/></Content></Page>".into(),
        }];
        let err = build_project(&files, "t").unwrap_err();
        assert!(matches!(err, ParseError::RootLayoutMissing));
    }

    #[test]
    fn unknown_model_reference_errors() {
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/leave/apply/page.kenvoy.xml".into(),
                content:
                    "<Page><Content><Form model=\"不存在\"><Field name=\"x\"/></Form></Content></Page>"
                        .into(),
            },
        ];
        let err = build_project(&files, "t").unwrap_err();
        assert!(matches!(err, ParseError::ModelNotFound { .. }));
    }
}
