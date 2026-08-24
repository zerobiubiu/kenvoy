//! DSL 解析：XML 业务语言 → [`crate::ir::ProjectIr`]。
//!
//! kengine 为无 I/O 纯解析引擎：输入「文件路径 + 内容」列表，输出 `IR`；
//! 文件系统扫描由 CLI（`kenvoy`）负责。

mod doc;
mod node;

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ir::{
    ComponentDef, Flow, IR_VERSION, Layout, Model, Node, Page, Permissions, ProjectIr, ProjectMeta,
    Role as IrRole,
};

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
    /// 页面引入了不存在的文件。
    #[error("{from}: 引入的文件 `{src}` 不存在")]
    ImportNotFound {
        /// 引入来源（页面路径）。
        from: String,
        /// 引入路径。
        src: String,
    },
    /// 引入路径非法。
    #[error("{from}: 引入路径 `{src}` 非法（不支持 `..`）")]
    ImportInvalid {
        /// 引入来源（页面路径）。
        from: String,
        /// 引入路径。
        src: String,
    },
    /// 引入的文件根元素既不是 `<Flow>` 也不是 `<Publish>`。
    #[error("{from}: 引入的文件 `{src}` 根元素 <{root}> 不受支持")]
    UnknownImportType {
        /// 引入来源（页面路径）。
        from: String,
        /// 引入路径。
        src: String,
        /// 根元素名。
        root: String,
    },
    /// `module/` 目录文件根元素不是 `<Component>`。
    #[error("{path}: module 文件根元素 <{root}> 不受支持（应为 <Component>）")]
    InvalidModuleRoot {
        /// 文件路径。
        path: String,
        /// 根元素名。
        root: String,
    },
    /// 组件名重复。
    #[error("组件名重复：`{name}`（页面内定义与 `module/` 目录冲突）")]
    DuplicateComponent {
        /// 重复的组件名。
        name: String,
    },
    /// 页面引用了未定义的流程。
    #[error("{from}: 引用的流程 `{flow}` 未定义（未由页面引入或不存在）")]
    FlowNotFound {
        /// 引用来源（页面路径）。
        from: String,
        /// 流程名。
        flow: String,
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
    /// 审批流文件（`flow.kenvoy.xml`，由页面显式引入）。
    Flow,
    /// 权限文件（`publish.kenvoy.xml`，由页面显式引入）。
    Publish,
    /// 自定义组件文件（`module/` 目录，根元素 `<Component>`）。
    Module,
    /// 翻译表文件（`I8n/<语言>/` 目录，根元素 `<Strings>`）。
    I18n,
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
    let mut flow_registry: BTreeMap<String, Flow> = BTreeMap::new();
    let mut components: BTreeMap<String, ComponentDef> = BTreeMap::new();
    let mut translations: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

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
                let page = doc::parse_page(&dir, root, files, &mut flow_registry, &mut components)?;
                if pages.iter().any(|p| p.path == page.path) {
                    return Err(ParseError::DuplicatePage { path: page.path });
                }
                pages.push(page);
            }
            Role::Module => {
                let def = doc::parse_component_file(&file.path, root)?;
                if components.contains_key(&def.name) {
                    return Err(ParseError::DuplicateComponent { name: def.name });
                }
                components.insert(def.name.clone(), def);
            }
            Role::I18n => {
                let (locale, strings) = doc::parse_strings(&file.path, root)?;
                translations.entry(locale).or_default().extend(strings);
            }
            Role::Flow | Role::Publish => {
                // 仅由页面 `<Import>` 显式引入，顶层不收集
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
    validate_components(&components, &models)?;
    validate_flows(&pages, &flow_registry)?;
    validate_charts(&pages, &models)?;

    let flows: Vec<Flow> = flow_registry.into_values().collect();
    let mut role_map: BTreeMap<String, IrRole> = BTreeMap::new();
    for publish in pages.iter().filter_map(|p| p.publish.as_ref()) {
        for role in &publish.roles {
            role_map
                .entry(role.name.clone())
                .or_insert_with(|| role.clone());
        }
    }
    let roles: Vec<IrRole> = role_map.into_values().collect();

    Ok(ProjectIr {
        version: IR_VERSION.to_string(),
        project: ProjectMeta {
            name: project_name.to_string(),
            description: None,
            locale: None,
        },
        models,
        layouts,
        pages,
        flows,
        permissions: Permissions { roles },
        components,
        translations,
    })
}

/// 依据文件路径识别角色与「apps 内目录路径」。
///
/// 例如 `apps/leave/apply/page.kenvoy.xml` → (`Page`, `/leave/apply`)；
/// `module/` 目录（全局或应用私有）下的 `*.kenvoy.xml` → `Module`；
/// `I8n/` 目录（不区分大小写）下的 `*.kenvoy.xml` → `I18n`，目录名即语言。
fn classify(path: &str) -> Option<(Role, String)> {
    let norm = path.replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    let (dir, file) = norm.rsplit_once('/')?;
    if lower.contains("/i8n/") || lower.starts_with("i8n/") {
        if file.ends_with(".kenvoy.xml") {
            return Some((Role::I18n, String::new()));
        }
        return None;
    }
    if norm.contains("/module/") || norm.starts_with("module/") {
        if file.ends_with(".kenvoy.xml") {
            return Some((Role::Module, String::new()));
        }
        return None;
    }
    let role = match file {
        "layout.kenvoy.xml" => Role::Layout,
        "page.kenvoy.xml" => Role::Page,
        "models.kenvoy.xml" => Role::Models,
        "flow.kenvoy.xml" => Role::Flow,
        "publish.kenvoy.xml" => Role::Publish,
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

/// 将页面 `<Import src>` 解析为项目内文件路径。
///
/// `src` 相对页面所在目录（如 `./flow.kenvoy.xml` → `apps/leave/apply/flow.kenvoy.xml`）；
/// 含 `..` 或空路径返回 `None`。
pub(crate) fn resolve_import_path(page_dir: &str, src: &str) -> Option<String> {
    let src = src.replace('\\', "/");
    let src = src.strip_prefix("./").unwrap_or(&src);
    if src.is_empty() || src.contains("..") {
        return None;
    }
    let base = if page_dir == "/" {
        "apps".to_string()
    } else {
        format!("apps{page_dir}")
    };
    Some(format!("{base}/{src}"))
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

/// 校验自定义组件定义体中的模型与字段引用。
fn validate_components(
    components: &BTreeMap<String, ComponentDef>,
    models: &BTreeMap<String, Model>,
) -> Result<(), ParseError> {
    for def in components.values() {
        validate_node(&def.root, models, &format!("组件 <{}-/>", def.name), None)?;
    }
    Ok(())
}

/// 校验页面引用的流程存在。
fn validate_flows(pages: &[Page], flows: &BTreeMap<String, Flow>) -> Result<(), ParseError> {
    for page in pages {
        if let Some(viewer) = find_flow_viewers(&page.content) {
            for flow in viewer {
                if !flows.contains_key(&flow) {
                    return Err(ParseError::FlowNotFound {
                        from: page.path.clone(),
                        flow,
                    });
                }
            }
        }
    }
    Ok(())
}

/// 收集页面树中的 `<FlowViewer>` 引用的流程名。
fn find_flow_viewers(node: &Node) -> Option<Vec<String>> {
    let mut out = Vec::new();
    collect_flow_viewers(node, &mut out);
    (!out.is_empty()).then_some(out)
}

fn collect_flow_viewers(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::FlowViewer { props } => out.push(props.flow.clone()),
        Node::Row { children, .. }
        | Node::Col { children, .. }
        | Node::Section { children, .. }
        | Node::Form { children, .. }
        | Node::Table { children, .. }
        | Node::Custom { children, .. } => {
            for child in children {
                collect_flow_viewers(child, out);
            }
        }
        Node::Slot
        | Node::Field { .. }
        | Node::Column { .. }
        | Node::StatCard { .. }
        | Node::Text { .. }
        | Node::Chart { .. } => {}
    }
}

/// 校验 `<Chart>` 引用的模型与字段存在。
fn validate_charts(pages: &[Page], models: &BTreeMap<String, Model>) -> Result<(), ParseError> {
    for page in pages {
        validate_chart_node(&page.content, models, &page.path)?;
    }
    Ok(())
}

fn validate_chart_node(
    node: &Node,
    models: &BTreeMap<String, Model>,
    from: &str,
) -> Result<(), ParseError> {
    match node {
        Node::Chart { props } => {
            let Some(m) = models.get(&props.model) else {
                return Err(ParseError::ModelNotFound {
                    from: from.to_string(),
                    model: props.model.clone(),
                });
            };
            if !m.fields.iter().any(|f| f.name == props.field) {
                return Err(ParseError::FieldNotFound {
                    from: from.to_string(),
                    model: props.model.clone(),
                    field: props.field.clone(),
                });
            }
        }
        Node::Row { children, .. }
        | Node::Col { children, .. }
        | Node::Section { children, .. }
        | Node::Form { children, .. }
        | Node::Table { children, .. }
        | Node::Custom { children, .. } => {
            for child in children {
                validate_chart_node(child, models, from)?;
            }
        }
        Node::Slot
        | Node::Field { .. }
        | Node::Column { .. }
        | Node::StatCard { .. }
        | Node::Text { .. }
        | Node::FlowViewer { .. } => {}
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
        Node::Slot
        | Node::Column { .. }
        | Node::StatCard { .. }
        | Node::Text { .. }
        | Node::Chart { .. }
        | Node::FlowViewer { .. } => {}
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
        assert_eq!(
            classify("apps/leave/apply/flow.kenvoy.xml"),
            Some((Role::Flow, "/leave/apply".to_string()))
        );
        assert_eq!(
            classify("apps/leave/apply/publish.kenvoy.xml"),
            Some((Role::Publish, "/leave/apply".to_string()))
        );
    }

    #[test]
    fn resolve_import_paths() {
        assert_eq!(
            resolve_import_path("/leave/apply", "./flow.kenvoy.xml"),
            Some("apps/leave/apply/flow.kenvoy.xml".to_string())
        );
        assert_eq!(
            resolve_import_path("/leave/apply", "flow.kenvoy.xml"),
            Some("apps/leave/apply/flow.kenvoy.xml".to_string())
        );
        assert_eq!(
            resolve_import_path("/", "./publish.kenvoy.xml"),
            Some("apps/publish.kenvoy.xml".to_string())
        );
        assert_eq!(resolve_import_path("/leave", "../flow.kenvoy.xml"), None);
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

    #[test]
    fn module_file_registers_component() {
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content: "<Page><Content><Text value=\"x\"/></Content></Page>".into(),
            },
            SourceFile {
                path: "module/组件/状态卡.kenvoy.xml".into(),
                content:
                    "<Component name=\"状态卡\"><StatCard data=\"{{data}}\" unit=\"次\"/></Component>"
                        .into(),
            },
        ];
        let ir = build_project(&files, "t").unwrap();
        let def = ir.components.get("状态卡").expect("组件未注册");
        assert!(matches!(def.root, Node::StatCard { .. }));
    }

    #[test]
    fn app_module_dir_registers_component() {
        // 应用私有 module/（apps/<应用>/module/）同样注册
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content: "<Page><Content><Text value=\"x\"/></Content></Page>".into(),
            },
            SourceFile {
                path: "apps/leave/module/审批卡片.kenvoy.xml".into(),
                content: "<Component name=\"审批卡片\"><Section title=\"审批\"><Slot/></Section></Component>"
                    .into(),
            },
        ];
        let ir = build_project(&files, "t").unwrap();
        assert!(ir.components.contains_key("审批卡片"));
    }

    #[test]
    fn inline_component_registers() {
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content:
                    "<Page><Component name=\"问候\"><Text value=\"你好 {{who}}\"/></Component>\
                <Content><问候 who=\"张三\"/></Content></Page>"
                        .into(),
            },
        ];
        let ir = build_project(&files, "t").unwrap();
        let def = ir.components.get("问候").expect("组件未注册");
        assert!(matches!(def.root, Node::Text { .. }));
        // 页面内容中 `<问候>` 是未知元素 → Custom
        let page = &ir.pages[0];
        assert!(matches!(&page.content, Node::Custom { component, .. } if component == "问候"));
    }

    #[test]
    fn duplicate_component_errors() {
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content: "<Page><Component name=\"卡\"><Text value=\"a\"/></Component>\
                <Content><Text value=\"x\"/></Content></Page>"
                    .into(),
            },
            SourceFile {
                path: "module/卡.kenvoy.xml".into(),
                content: "<Component name=\"卡\"><Text value=\"b\"/></Component>".into(),
            },
        ];
        let err = build_project(&files, "t").unwrap_err();
        assert!(matches!(err, ParseError::DuplicateComponent { .. }));
    }

    #[test]
    fn module_file_with_wrong_root_errors() {
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content: "<Page><Content><Text value=\"x\"/></Content></Page>".into(),
            },
            SourceFile {
                path: "module/坏.kenvoy.xml".into(),
                content: "<Layout><Root/></Layout>".into(),
            },
        ];
        let err = build_project(&files, "t").unwrap_err();
        assert!(matches!(err, ParseError::InvalidModuleRoot { .. }));
    }

    #[test]
    fn component_body_validates_models() {
        // 组件定义体内的 Form 引用不存在的模型 → 报错
        let files = vec![
            SourceFile {
                path: "apps/layout.kenvoy.xml".into(),
                content: "<Layout><Root><Slot/></Root></Layout>".into(),
            },
            SourceFile {
                path: "apps/dashboard/page.kenvoy.xml".into(),
                content: "<Page><Content><Text value=\"x\"/></Content></Page>".into(),
            },
            SourceFile {
                path: "module/表单卡.kenvoy.xml".into(),
                content: "<Component name=\"表单卡\"><Form model=\"不存在\"></Form></Component>"
                    .into(),
            },
        ];
        let err = build_project(&files, "t").unwrap_err();
        assert!(matches!(err, ParseError::ModelNotFound { .. }));
    }
}
