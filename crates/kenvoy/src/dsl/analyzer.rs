use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
};

use kengine::{DependencyKind, ModuleBindingTarget, ProgramIR, ResolvedModule};
use serde_json::Value;

use crate::{application::*, validation};

use super::{
    DslDiagnostic, DslDiagnosticCategory, SourceLocation,
    static_eval::{
        Evaluator, ExpressionReference, ResolvedReference, StaticElement, StaticNode, StaticValue,
    },
};

#[derive(Debug)]
pub struct AnalyzeResult {
    pub application: Option<ApplicationIR>,
    pub diagnostics: Vec<DslDiagnostic>,
}

impl AnalyzeResult {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

type Result<T> = std::result::Result<T, DslDiagnostic>;

pub fn analyze(program: &ProgramIR, entry: &Path, default_name: &str) -> AnalyzeResult {
    let nodes = match Evaluator::new(program).entry(entry) {
        Ok(nodes) => nodes,
        Err(error) => {
            return AnalyzeResult {
                application: None,
                diagnostics: vec![error],
            };
        }
    };
    let entry_source = program
        .modules
        .iter()
        .find(|module| module.path == entry)
        .map(|module| SourceLocation {
            path: entry.into(),
            span: module
                .exports
                .first()
                .map(|export| export.span.clone())
                .unwrap_or(kengine::SourceSpan {
                    start: 0,
                    end: 0,
                    start_position: kengine::Position { line: 1, column: 1 },
                    end_position: kengine::Position { line: 1, column: 1 },
                }),
        })
        .unwrap_or_else(|| SourceLocation {
            path: entry.into(),
            span: start_of_file(),
        });
    let mut analyzer = Analyzer {
        program,
        entry_source,
        resources: BTreeMap::new(),
        pages: vec![],
        actions: vec![],
        navigation: vec![],
        permissions: vec![],
        modules: vec![],
        component_overrides: BTreeMap::new(),
        diagnostics: vec![],
    };
    let name = match analyzer.root(&nodes, default_name) {
        Ok(name) => name,
        Err(error) => {
            analyzer.diagnostics.push(error);
            return AnalyzeResult {
                application: None,
                diagnostics: analyzer.diagnostics,
            };
        }
    };
    analyzer.check_boundaries();
    if !analyzer.diagnostics.is_empty() {
        return AnalyzeResult {
            application: None,
            diagnostics: analyzer.diagnostics,
        };
    }
    for resource in analyzer.resources.values() {
        for operation in [
            Operation::Create,
            Operation::List,
            Operation::Read,
            Operation::Update,
            Operation::Delete,
        ] {
            analyzer.permissions.push(PermissionDefinition {
                target: PermissionTarget::Resource(resource.name.clone()),
                operation,
                role: Role::Admin,
                fields: resource
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
                form: None,
            });
        }
    }
    AnalyzeResult {
        application: Some(ApplicationIR {
            format_version: APPLICATION_IR_VERSION,
            name,
            resources: analyzer.resources.into_values().collect(),
            pages: analyzer.pages,
            actions: analyzer.actions,
            navigation: analyzer.navigation,
            permissions: analyzer.permissions,
            module_references: analyzer.modules,
            component_overrides: analyzer.component_overrides,
        }),
        diagnostics: vec![],
    }
}

struct Analyzer<'a> {
    program: &'a ProgramIR,
    entry_source: SourceLocation,
    resources: BTreeMap<String, ResourceDefinition>,
    pages: Vec<PageDefinition>,
    actions: Vec<ActionDefinition>,
    navigation: Vec<NavigationDefinition>,
    permissions: Vec<PermissionDefinition>,
    modules: Vec<ModuleReference>,
    component_overrides: BTreeMap<String, String>,
    diagnostics: Vec<DslDiagnostic>,
}

fn error(
    element: &StaticElement,
    code: &str,
    message: impl Into<String>,
    suggestion: &str,
) -> DslDiagnostic {
    semantic_error(element.source.clone(), code, message, suggestion)
}

fn semantic_error(
    source: SourceLocation,
    code: &str,
    message: impl Into<String>,
    suggestion: &str,
) -> DslDiagnostic {
    let mut diagnostic = DslDiagnostic::new(code, source, message, suggestion);
    diagnostic.category = DslDiagnosticCategory::Semantic;
    diagnostic
}

fn elements(nodes: &[StaticNode]) -> Result<Vec<&StaticElement>> {
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

fn attrs(element: &StaticElement, allowed: &[&str]) -> Result<()> {
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

fn json<'a>(element: &'a StaticElement, name: &str) -> Result<Option<&'a Value>> {
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

fn string(element: &StaticElement, name: &str) -> Result<Option<String>> {
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

fn required_string(element: &StaticElement, name: &str) -> Result<String> {
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

fn boolean(element: &StaticElement, name: &str) -> Result<bool> {
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

fn number(element: &StaticElement, name: &str) -> Result<Option<f64>> {
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

fn positive_integer(element: &StaticElement, name: &str, limit: u64) -> Result<Option<u64>> {
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

fn list(element: &StaticElement, name: &str) -> Result<Option<Vec<String>>> {
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

fn identifier(name: &str) -> bool {
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

fn route(path: &str) -> bool {
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

fn role(element: &StaticElement, required: bool) -> Result<Role> {
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

fn field_kind(name: &str) -> Option<FieldKind> {
    match name {
        "text" => Some(FieldKind::Text),
        "textarea" => Some(FieldKind::Textarea),
        "number" => Some(FieldKind::Number),
        "boolean" => Some(FieldKind::Boolean),
        "date" => Some(FieldKind::Date),
        "datetime" => Some(FieldKind::Datetime),
        "select" => Some(FieldKind::Select),
        "file" => Some(FieldKind::File),
        _ => None,
    }
}

fn field(element: &StaticElement) -> Result<FieldDefinition> {
    intrinsic(element)?;
    let kind = field_kind(&element.name).ok_or_else(|| {
        error(
            element,
            "KV2001",
            format!("Resource 内不允许标签 {}", element.name),
            "Resource 只允许字段和静态 Schema Component",
        )
    })?;
    let mut allowed = vec!["name", "label", "required", "default"];
    if kind != FieldKind::File {
        allowed.push("index");
    }
    match kind {
        FieldKind::Text | FieldKind::Textarea => allowed.extend(["maxLength", "unique"]),
        FieldKind::Number => allowed.extend(["integer", "min", "max", "unique"]),
        FieldKind::Select => allowed.push("optionsSource"),
        FieldKind::File => allowed.extend(["accept", "maxSize", "maxFiles"]),
        _ => {}
    }
    attrs(element, &allowed)?;
    let name = required_string(element, "name")?;
    if !identifier(&name) {
        return Err(error(
            element,
            "KV2006",
            format!("字段名 {name} 非法或保留"),
            "使用 a-z 开头的小写字母、数字和下划线，避开系统字段",
        ));
    }
    let max_length = number(element, "maxLength")?
        .map(|value| {
            if value < 0.0 || value.fract() != 0.0 || value > 1_000_000.0 {
                Err(error(
                    element,
                    "KV2007",
                    "maxLength 必须为 0 到 1000000 的整数",
                    "采用受控的非负长度上限",
                ))
            } else {
                Ok(value as usize)
            }
        })
        .transpose()?;
    let upload = if kind == FieldKind::File {
        let defaults = FileConstraints::default();
        let constraints = FileConstraints {
            accept: list(element, "accept")?.unwrap_or_default(),
            max_size: positive_integer(element, "maxSize", MAX_UPLOAD_SIZE)?
                .unwrap_or(defaults.max_size),
            max_files: positive_integer(element, "maxFiles", MAX_UPLOAD_FILES as u64)?
                .map(|value| value as usize)
                .unwrap_or(defaults.max_files),
        };
        validation::validate_file_constraints(&constraints).map_err(|message| {
            error(element, "KV2007", message, "检查文件数量、大小与 MIME 限制")
        })?;
        Some(constraints)
    } else {
        None
    };
    let mut definition = FieldDefinition {
        label: string(element, "label")?.unwrap_or_else(|| name.clone()),
        name,
        kind,
        required: boolean(element, "required")?,
        default: json(element, "default")?.cloned(),
        integer: boolean(element, "integer")?,
        min: number(element, "min")?,
        max: number(element, "max")?,
        max_length,
        unique: boolean(element, "unique")?,
        index: boolean(element, "index")?,
        options: vec![],
        options_source: json(element, "optionsSource")?.map(|value| serde_json::from_value(value.clone()).map_err(|_| error(element,"KV2015","optionsSource 必须声明 resource/value/label/permission，可选 filters 为源字段到本资源字段的映射","使用静态对象；permission 显式取 anonymous/user/admin"))).transpose()?,
        upload,
        source: element.source.clone(),
    };
    if definition
        .min
        .zip(definition.max)
        .is_some_and(|(min, max)| min > max)
        || (definition.required && definition.max_length == Some(0))
        || (definition.integer
            && definition
                .min
                .unwrap_or(-validation::MAX_SAFE_INTEGER)
                .max(-validation::MAX_SAFE_INTEGER)
                .ceil()
                > definition
                    .max
                    .unwrap_or(validation::MAX_SAFE_INTEGER)
                    .min(validation::MAX_SAFE_INTEGER)
                    .floor())
    {
        return Err(error(
            element,
            "KV2007",
            "字段约束的允许值范围为空",
            "检查 min/max、integer 安全范围以及 required/maxLength 的组合",
        ));
    }
    if kind == FieldKind::Select {
        let mut values = BTreeSet::new();
        for option in elements(&element.children)? {
            intrinsic(option)?;
            if option.name != "option" {
                return Err(error(
                    option,
                    "KV2001",
                    "select 只允许 option 子节点",
                    "为每个枚举值声明 option",
                ));
            }
            attrs(option, &["value", "label", "when"])?;
            no_children(option)?;
            let value = required_string(option, "value")?;
            if value.contains('\0') {
                return Err(error(
                    option,
                    "KV2007",
                    "选项值不能包含 NUL 字符",
                    "使用不含 NUL 的字符串，保证声明、表单验证与 SQLite 存储一致",
                ));
            }
            if !values.insert(value.clone()) {
                return Err(error(
                    option,
                    "KV2005",
                    format!("重复选项值 {value}"),
                    "为 option 提供唯一 value",
                ));
            }
            definition.options.push(SelectOption {
                when: json(option, "when")?
                    .map(|value| {
                        serde_json::from_value(value.clone()).map_err(|_| {
                            error(
                                option,
                                "KV2015",
                                "option.when 必须是字段到标量的对象",
                                "使用静态对象声明联动条件",
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or_default(),
                label: string(option, "label")?.unwrap_or_else(|| value.clone()),
                value,
            });
        }
        if definition.options.is_empty() && definition.options_source.is_none() {
            return Err(error(
                element,
                "KV2007",
                "select 至少需要一个 option 或 optionsSource",
                "声明静态字符串选项或受控资源来源",
            ));
        }
    } else {
        no_children(element)?;
    }
    if let Some(default) = &definition.default {
        definition.default = Some(validation::validate_default(&definition, default).map_err(
            |failure| {
                error(
                    element,
                    "KV2008",
                    format!("默认值无效：{failure}"),
                    "默认值必须满足与提交数据相同的字段约束",
                )
            },
        )?);
    }
    Ok(definition)
}

fn no_children(element: &StaticElement) -> Result<()> {
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

fn intrinsic(element: &StaticElement) -> Result<()> {
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

fn unique_fields(fields: &[FieldDefinition]) -> Result<()> {
    let mut seen: BTreeMap<&str, &SourceLocation> = BTreeMap::new();
    for field in fields {
        if let Some(previous) = seen.insert(&field.name, &field.source) {
            let mut failure = semantic_error(
                field.source.clone(),
                "KV2005",
                format!("重复字段 {}", field.name),
                "检查当前资源及展开的 Schema Component",
            );
            failure.related.push(previous.clone());
            return Err(failure);
        }
    }
    Ok(())
}

impl Analyzer<'_> {
    fn root(&mut self, nodes: &[StaticNode], default_name: &str) -> Result<String> {
        let roots = elements(nodes)?;
        if roots.len() != 1 {
            let source = roots
                .first()
                .map(|item| item.source.clone())
                .or_else(|| nodes.first().map(source_of))
                .unwrap_or_else(|| self.entry_source.clone());
            return Err(semantic_error(
                source,
                "KV2001",
                "默认导出必须恰好包含一个 app 或 page 根",
                "使用单个 app 包含所有声明",
            ));
        }
        let root = roots[0];
        intrinsic(root)?;
        let (name, children) = if root.name == "app" {
            attrs(root, &["name"])?;
            (required_string(root, "name")?, elements(&root.children)?)
        } else if root.name == "page" {
            (default_name.to_owned(), vec![root])
        } else {
            return Err(error(
                root,
                "KV2001",
                format!("不支持的应用根 {}", root.name),
                "入口默认导出 app 或 page 声明",
            ));
        };
        // 显式资源先登记，避免源码顺序改变推导规则。
        for child in &children {
            if child.name == "resource"
                && let Err(error) = self.explicit_resource(child)
            {
                self.diagnostics.push(error);
            }
        }
        let mut raw_pages = Vec::new();
        let mut raw_navigation = Vec::new();
        for child in children {
            match child.name.as_str() {
                "resource" => {}
                "page" => match self.prepare_page(child) {
                    Ok(page) => raw_pages.push(page),
                    Err(error) => self.diagnostics.push(error),
                },
                "action" => {
                    if let Err(error) = self.action_definition(child) {
                        self.diagnostics.push(error);
                    }
                }
                "navigation" => raw_navigation.push(child),
                "override" => {
                    if let Err(error) = self.override_definition(child) {
                        self.diagnostics.push(error);
                    }
                }
                _ => self.diagnostics.push(error(
                    child,
                    "KV2001",
                    format!("app 内不允许标签 {}", child.name),
                    "app 只包含 resource、page、action、navigation 或 override",
                )),
            }
        }
        // 所有显式/隐式资源存在后，再解析 form/table 的引用。
        for prepared in raw_pages {
            if let Err(error) = self.finish_page(prepared) {
                self.diagnostics.push(error);
            }
        }
        for navigation in raw_navigation {
            match self.navigation_definition(navigation) {
                Ok(definition) => self.navigation.push(definition),
                Err(error) => self.diagnostics.push(error),
            }
        }
        if let Err((source, message)) = crate::options::validate_definitions(
            &self.resources.values().cloned().collect::<Vec<_>>(),
        ) {
            self.diagnostics.push(semantic_error(
                source,
                "KV2015",
                message,
                "检查选项来源、联动字段及依赖循环",
            ));
        }
        for policy in &self.permissions {
            let PermissionTarget::Resource(name) = &policy.target else {
                continue;
            };
            let Some(resource) = self.resources.get(name) else {
                continue;
            };
            for field in &resource.fields {
                if let Some(source) = &field.options_source
                    && (policy.fields.contains(&field.name)
                        || field.default.as_ref().is_some_and(|value| !value.is_null()))
                    && policy.role != Role::Admin
                    && source.permission != Role::Anonymous
                    && policy.role != source.permission
                {
                    self.diagnostics.push(semantic_error(
                        field.source.clone(),
                        "KV2015",
                        format!("表单权限不足以查询或提交字段 {} 的选项来源", field.name),
                        "提高页面权限、缩小表单字段范围，或明确降低来源投影权限",
                    ));
                }
            }
        }
        Ok(name)
    }

    fn explicit_resource(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name"])?;
        let name = required_string(element, "name")?;
        if !identifier(&name) {
            return Err(error(
                element,
                "KV2006",
                format!("资源名 {name} 非法或保留"),
                "使用 a-z 开头的小写字母、数字和下划线",
            ));
        }
        if let Some(previous) = self.resources.get(&name) {
            let mut failure = error(
                element,
                "KV2005",
                format!("重复 Resource {name}"),
                "同名显式 Resource 只能声明一次",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let fields = elements(&element.children)?
            .into_iter()
            .map(field)
            .collect::<Result<Vec<_>>>()?;
        unique_fields(&fields)?;
        if fields.is_empty() {
            return Err(error(
                element,
                "KV2007",
                "Resource 至少需要一个字段",
                "声明静态字段；不要用动态 JSX 生成 Schema",
            ));
        }
        self.resources.insert(
            name.clone(),
            ResourceDefinition {
                name,
                fields,
                source: element.source.clone(),
            },
        );
        Ok(())
    }

    fn prepare_page<'a>(&mut self, element: &'a StaticElement) -> Result<PreparedPage<'a>> {
        intrinsic(element)?;
        attrs(element, &["path", "title", "resource", "permission"])?;
        let path = required_string(element, "path")?;
        if !route(&path) {
            return Err(error(
                element,
                "KV2009",
                format!("页面路径 {path} 非法或占用保留命名空间"),
                "使用规范静态绝对路径，避开 /api、/admin 和 /_kenvoy",
            ));
        }
        let mut fields = vec![];
        collect_page_fields(&element.children, &mut fields)?;
        unique_fields(&fields)?;
        let resource = if fields.is_empty() {
            if element.attributes.contains_key("resource") {
                return Err(error(
                    element,
                    "KV2007",
                    "page.resource 仅用于页面字段简写",
                    "已有 Resource 请使用 form/table 的 resource 属性",
                ));
            }
            None
        } else {
            if has_control(&element.children) {
                return Err(error(
                    element,
                    "KV2007",
                    "页面字段简写不能与显式 form/table 混用",
                    "将字段移入显式 Resource 并使用 form/table",
                ));
            }
            let name = match string(element, "resource")? {
                Some(name) => name,
                None if path.matches('/').count() == 1 => path.trim_start_matches('/').into(),
                None => {
                    return Err(error(
                        element,
                        "KV2007",
                        "多段路径的字段简写必须明确 page.resource",
                        "为多个页面指定同一个稳定资源名，避免意外创建不同数据表",
                    ));
                }
            };
            if !identifier(&name) {
                return Err(error(
                    element,
                    "KV2006",
                    format!("无法使用资源名 {name}"),
                    "显式指定符合标识符规范的 page.resource",
                ));
            }
            if let Some(previous) = self.resources.get(&name) {
                if fields.len() != previous.fields.len()
                    || !fields.iter().all(|field| {
                        previous
                            .field(&field.name)
                            .is_some_and(|old| old.same_schema(field))
                    })
                {
                    let mut failure = error(
                        element,
                        "KV2010",
                        format!("页面字段与 Resource {name} 的完整结构不一致"),
                        "复用显式 form 或提供相同字段、类型、默认值和约束，不能静默合并",
                    );
                    failure.related.push(previous.source.clone());
                    return Err(failure);
                }
            } else {
                self.resources.insert(
                    name.clone(),
                    ResourceDefinition {
                        name: name.clone(),
                        fields: fields.clone(),
                        source: element.source.clone(),
                    },
                );
            }
            Some(name)
        };
        Ok(PreparedPage {
            element,
            path,
            resource,
            fields,
        })
    }

    fn finish_page(&mut self, prepared: PreparedPage<'_>) -> Result<()> {
        if let Some(previous) = self.pages.iter().find(|page| page.path == prepared.path) {
            let mut failure = error(
                prepared.element,
                "KV2009",
                format!("重复页面路径 {}", prepared.path),
                "每个页面使用唯一规范路径",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let permission = role(prepared.element, false)?;
        let mut state = PageState {
            path: &prepared.path,
            permission,
            auto_resource: prepared.resource.as_deref(),
            auto_fields: &prepared.fields,
            inserted_form: false,
            form_count: 0,
        };
        let components = self.components(&prepared.element.children, &mut state)?;
        self.pages.push(PageDefinition {
            title: string(prepared.element, "title")?.unwrap_or_else(|| prepared.path.clone()),
            path: prepared.path,
            permission,
            components,
            source: prepared.element.source.clone(),
        });
        Ok(())
    }

    fn components(
        &mut self,
        nodes: &[StaticNode],
        state: &mut PageState<'_>,
    ) -> Result<Vec<ComponentDefinition>> {
        let mut output = vec![];
        for node in nodes {
            let element = match node {
                StaticNode::Text { value, source } => {
                    if !value.trim().is_empty() {
                        output.push(ComponentDefinition::Text {
                            value: value.clone(),
                            source: source.clone(),
                        });
                    }
                    continue;
                }
                StaticNode::Element(element) => element,
            };
            if field_kind(&element.name).is_some() && element.module_ref.is_none() {
                if !state.inserted_form {
                    let resource = state.auto_resource.ok_or_else(|| {
                        error(
                            element,
                            "KV2001",
                            "自定义组件内部不能声明数据库字段",
                            "将业务字段放在 Resource 或页面字段组",
                        )
                    })?;
                    state.inserted_form = true;
                    output.push(
                        self.make_form(
                            element,
                            resource.into(),
                            FormMode::Create,
                            state
                                .auto_fields
                                .iter()
                                .map(|field| field.name.clone())
                                .collect(),
                            state,
                        ),
                    );
                }
                continue;
            }
            let component = match element.name.as_str() {
                "form" if element.module_ref.is_none() => {
                    attrs(element, &["resource", "mode", "fields"])?;
                    no_children(element)?;
                    let resource = required_string(element, "resource")?;
                    let definition = self.resources.get(&resource).ok_or_else(|| {
                        error(
                            element,
                            "KV2011",
                            format!("未知 Resource {resource}"),
                            "先声明 Resource 或由其他页面明确推导",
                        )
                    })?;
                    let mode = match string(element, "mode")?.as_deref() {
                        None | Some("create") => FormMode::Create,
                        Some("edit") => FormMode::Edit,
                        _ => {
                            return Err(error(
                                element,
                                "KV2004",
                                "form.mode 只支持 create 或 edit",
                                "选择明确的表单操作",
                            ));
                        }
                    };
                    let fields = list(element, "fields")?.unwrap_or_else(|| {
                        definition
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect()
                    });
                    check_projection(element, definition, &fields)?;
                    if definition
                        .fields
                        .iter()
                        .filter(|field| fields.contains(&field.name))
                        .any(|field| {
                            field
                                .option_dependencies()
                                .iter()
                                .any(|dependency| !fields.iter().any(|name| name == dependency))
                        })
                    {
                        return Err(error(
                            element,
                            "KV2015",
                            "表单 fields 必须包含选项联动所需字段",
                            "将联动父字段加入同一表单，不能跨表单拼接权限",
                        ));
                    }
                    if mode == FormMode::Create
                        && definition.fields.iter().any(|field| {
                            field.required
                                && field.default.is_none()
                                && !fields.contains(&field.name)
                        })
                    {
                        return Err(error(
                            element,
                            "KV2007",
                            "创建表单遗漏没有默认值的必填字段",
                            "将必填字段加入 fields 或提供有效默认值",
                        ));
                    }
                    self.make_form(element, resource, mode, fields, state)
                }
                "table" if element.module_ref.is_none() => {
                    attrs(element, &["resource", "columns"])?;
                    no_children(element)?;
                    let resource = required_string(element, "resource")?;
                    let definition = self.resources.get(&resource).ok_or_else(|| {
                        error(
                            element,
                            "KV2011",
                            format!("未知 Resource {resource}"),
                            "先声明 Resource",
                        )
                    })?;
                    let columns = list(element, "columns")?.unwrap_or_else(|| {
                        definition
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect()
                    });
                    check_projection(element, definition, &columns)?;
                    ComponentDefinition::Table {
                        resource,
                        columns,
                        source: element.source.clone(),
                    }
                }
                "group" if element.module_ref.is_none() => {
                    attrs(element, &["label"])?;
                    ComponentDefinition::Group {
                        label: string(element, "label")?,
                        children: self.components(&element.children, state)?,
                        source: element.source.clone(),
                    }
                }
                "image" if element.module_ref.is_none() => {
                    attrs(element, &["src", "alt", "width", "height", "fit"])?;
                    no_children(element)?;
                    let src = required_string(element, "src")?;
                    if !validation::valid_image_src(&src) {
                        return Err(error(
                            element,
                            "KV2007",
                            "image.src 必须为无凭据的 HTTP(S) 绝对 URL 或单斜杠开头的站内路径",
                            "不能使用 //、反斜杠、控制字符或 javascript/data/blob URL",
                        ));
                    }
                    let alt = string(element, "alt")?.ok_or_else(|| {
                        error(
                            element,
                            "KV2004",
                            "image 必须显式声明 alt，可使用空字符串",
                            "提供图片说明；装饰图使用 alt=''",
                        )
                    })?;
                    let fit = string(element, "fit")?.unwrap_or_else(|| "contain".into());
                    if !matches!(fit.as_str(), "contain" | "cover") {
                        return Err(error(
                            element,
                            "KV2007",
                            "image.fit 只允许 contain 或 cover",
                            "明确选择图片适配方式",
                        ));
                    }
                    ComponentDefinition::Image {
                        src,
                        alt,
                        width: positive_integer(element, "width", 4096)?.map(|value| value as u32),
                        height: positive_integer(element, "height", 4096)?
                            .map(|value| value as u32),
                        fit,
                        source: element.source.clone(),
                    }
                }
                _ => {
                    let reference = element.module_ref.as_ref().ok_or_else(|| {
                        error(
                            element,
                            "KV2001",
                            format!("页面中未知标签 {}", element.name),
                            "使用内置 form/table/group/image 或导入普通 UI 组件",
                        )
                    })?;
                    let module = self.register_module(
                        reference,
                        ModuleReferenceKind::ClientComponent,
                        &element.source,
                        None,
                    );
                    let mut props = BTreeMap::new();
                    for (name, value) in &element.attributes {
                        let prop = match value {
                            StaticValue::Json(value) => PropValue::Literal(value.clone()),
                            StaticValue::Reference(reference) => {
                                PropValue::Module(self.register_module(
                                    reference,
                                    ModuleReferenceKind::ClientFunction,
                                    &element.source,
                                    None,
                                ))
                            }
                            StaticValue::Expression(reference) => PropValue::Module(
                                self.register_expression(reference, &element.source),
                            ),
                        };
                        props.insert(name.clone(), prop);
                    }
                    // UI children 仍由页面组件协议处理，但不能隐式贡献 Resource 字段。
                    let mut child_state = PageState {
                        path: state.path,
                        permission: state.permission,
                        auto_resource: None,
                        auto_fields: &[],
                        inserted_form: false,
                        form_count: state.form_count,
                    };
                    let children = self.components(&element.children, &mut child_state)?;
                    state.form_count = child_state.form_count;
                    ComponentDefinition::Custom {
                        module,
                        props,
                        children,
                        source: element.source.clone(),
                    }
                }
            };
            output.push(component);
        }
        Ok(output)
    }

    fn make_form(
        &mut self,
        element: &StaticElement,
        resource: String,
        mode: FormMode,
        fields: Vec<String>,
        state: &mut PageState<'_>,
    ) -> ComponentDefinition {
        state.form_count += 1;
        let id = format!("{}#form-{}", state.path, state.form_count);
        if mode == FormMode::Create && state.permission != Role::Admin {
            self.permissions.push(PermissionDefinition {
                target: PermissionTarget::Resource(resource.clone()),
                operation: Operation::Create,
                role: state.permission,
                fields: fields.clone(),
                form: Some(id.clone()),
            });
        }
        let labels = if state.auto_resource == Some(resource.as_str()) {
            state
                .auto_fields
                .iter()
                .map(|field| (field.name.clone(), field.label.clone()))
                .collect()
        } else {
            BTreeMap::new()
        };
        ComponentDefinition::Form {
            id,
            resource,
            mode,
            fields,
            labels,
            source: element.source.clone(),
        }
    }

    fn register_module(
        &mut self,
        reference: &ResolvedReference,
        kind: ModuleReferenceKind,
        source: &SourceLocation,
        expression: Option<kengine::SourceSpan>,
    ) -> String {
        if let Some(existing) = self.modules.iter().find(|module| {
            module.module == reference.module
                && module.export == reference.export
                && module.kind == kind
                && module.expression == expression
        }) {
            return existing.id.clone();
        }
        let id = format!("module_{}", self.modules.len() + 1);
        self.modules.push(ModuleReference {
            id: id.clone(),
            module: reference.module.clone(),
            export: reference.export.clone(),
            external: reference.external,
            kind,
            expression,
            source: source.clone(),
        });
        id
    }

    fn register_expression(
        &mut self,
        reference: &ExpressionReference,
        source: &SourceLocation,
    ) -> String {
        self.register_module(
            &ResolvedReference {
                module: reference.module.clone(),
                export: String::new(),
                external: false,
            },
            ModuleReferenceKind::ClientExpression,
            source,
            Some(reference.span.clone()),
        )
    }

    fn override_definition(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name", "component"])?;
        no_children(element)?;
        let name = required_string(element, "name")?;
        if !matches!(
            name.as_str(),
            "text"
                | "textarea"
                | "number"
                | "boolean"
                | "date"
                | "datetime"
                | "select"
                | "file"
                | "image"
                | "form"
                | "table"
                | "group"
                | "page"
        ) {
            return Err(error(
                element,
                "KV2016",
                "不支持的系统组件重载目标",
                "使用字段类型或 image/form/table/group/page",
            ));
        }
        if self.component_overrides.contains_key(&name) {
            return Err(error(
                element,
                "KV2005",
                format!("重复组件重载 {name}"),
                "每个系统组件只注册一个重载",
            ));
        }
        let reference = match element.attributes.get("component") {
            Some(StaticValue::Reference(reference))
                if reference.external || self.is_function(reference) =>
            {
                reference
            }
            _ => {
                return Err(error(
                    element,
                    "KV2016",
                    "重载 component 必须引用导出的 React 函数组件",
                    "传入组件引用，不传元素、对象、字符串或调用结果",
                ));
            }
        };
        let id = self.register_module(
            reference,
            ModuleReferenceKind::ClientComponent,
            &element.source,
            None,
        );
        self.component_overrides.insert(name, id);
        Ok(())
    }

    fn action_definition(&mut self, element: &StaticElement) -> Result<()> {
        intrinsic(element)?;
        attrs(element, &["name", "handler", "runtime", "permission"])?;
        no_children(element)?;
        let name = required_string(element, "name")?;
        if !action_identifier(&name) {
            return Err(error(
                element,
                "KV2006",
                format!("Action 名称 {name} 非法"),
                "使用字母开头的 ASCII 字母、数字或下划线",
            ));
        }
        if let Some(previous) = self.actions.iter().find(|action| action.name == name) {
            let mut failure = error(
                element,
                "KV2005",
                format!("重复 Action {name}"),
                "每个 Action 使用唯一名称",
            );
            failure.related.push(previous.source.clone());
            return Err(failure);
        }
        let runtime = match required_string(element, "runtime")?.as_str() {
            "server" => ExecutionEnvironment::Server,
            "client" => ExecutionEnvironment::Client,
            "shared" => ExecutionEnvironment::Shared,
            _ => {
                return Err(error(
                    element,
                    "KV2004",
                    "Action runtime 必须为 server/client/shared",
                    "明确函数执行位置",
                ));
            }
        };
        let permission = role(element, true)?;
        let reference = match element.attributes.get("handler") {
            Some(StaticValue::Reference(reference)) => reference,
            _ => {
                return Err(error(
                    element,
                    "KV2012",
                    "Action handler 必须是可定位的模块导出函数引用",
                    "导入独立模块导出的纯函数，不能传字符串或执行函数调用",
                ));
            }
        };
        if reference.external && runtime != ExecutionEnvironment::Client {
            return Err(error(
                element,
                "KV2012",
                "服务端或共享 Action 不直接执行外部包函数",
                "使用受控本地模块导出可信纯函数",
            ));
        }
        if !reference.external && !self.is_function(reference) {
            return Err(error(
                element,
                "KV2012",
                "Action handler 导出不是函数声明或函数初始值",
                "导出普通函数或 const 箭头函数；不使用类、对象或运行时工厂",
            ));
        }
        let kind = match runtime {
            ExecutionEnvironment::Client => ModuleReferenceKind::ClientFunction,
            ExecutionEnvironment::Server => ModuleReferenceKind::ServerFunction,
            ExecutionEnvironment::Shared => ModuleReferenceKind::SharedFunction,
        };
        let handler = self.register_module(reference, kind, &element.source, None);
        self.permissions.push(PermissionDefinition {
            target: PermissionTarget::Action(name.clone()),
            operation: Operation::Execute,
            role: permission,
            fields: vec![],
            form: None,
        });
        self.actions.push(ActionDefinition {
            name,
            runtime,
            permission,
            handler,
            source: element.source.clone(),
        });
        Ok(())
    }

    fn navigation_definition(&self, element: &StaticElement) -> Result<NavigationDefinition> {
        intrinsic(element)?;
        attrs(element, &["label"])?;
        Ok(NavigationDefinition {
            label: string(element, "label")?,
            items: self.navigation_items(&element.children)?,
            source: element.source.clone(),
        })
    }

    fn is_function(&self, reference: &ResolvedReference) -> bool {
        let targets = self
            .program
            .module_bindings
            .exports
            .iter()
            .find(|export| {
                export.module == reference.module
                    && export.name == reference.export
                    && export.namespace == kengine::SymbolNamespace::Value
            })
            .map(|export| export.targets.as_slice())
            .unwrap_or_default();
        let [target] = targets else {
            return false;
        };
        match target {
            ModuleBindingTarget::Symbol { module, symbol } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| {
                    module.semantic.symbol(*symbol).is_some_and(|symbol| {
                        symbol.kind == kengine::SymbolKind::Function
                            || (symbol.kind == kengine::SymbolKind::Const
                                && module
                                    .declarations
                                    .iter()
                                    .flat_map(|decl| &decl.declarators)
                                    .any(|decl| {
                                        symbol.declarations.iter().any(|span| {
                                            decl.pattern.span.start <= span.start
                                                && decl.pattern.span.end >= span.end
                                        }) && decl
                                            .initializer
                                            .is_some_and(|id| function_expression(module, id))
                                    }))
                    })
                }),
            ModuleBindingTarget::Expression { module, expression } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| function_expression(module, *expression)),
            ModuleBindingTarget::Declaration { module, span } => self
                .program
                .modules
                .iter()
                .find(|item| &item.path == module)
                .is_some_and(|module| {
                    module.declarations.iter().any(|decl| {
                        decl.kind == kengine::DeclarationKind::Function
                            && decl.span.start >= span.start
                            && decl.span.end <= span.end
                    })
                }),
            _ => false,
        }
    }

    fn navigation_items(&self, nodes: &[StaticNode]) -> Result<Vec<NavigationItem>> {
        elements(nodes)?
            .into_iter()
            .map(|element| {
                intrinsic(element)?;
                match element.name.as_str() {
                    "item" => {
                        attrs(element, &["label", "path"])?;
                        no_children(element)?;
                        let path = required_string(element, "path")?;
                        let page = self
                            .pages
                            .iter()
                            .find(|page| page.path == path)
                            .ok_or_else(|| {
                                error(
                                    element,
                                    "KV2011",
                                    format!("导航目标 {path} 不存在"),
                                    "导航只引用已声明页面",
                                )
                            })?;
                        Ok(NavigationItem::Link {
                            label: required_string(element, "label")?,
                            path,
                            permission: page.permission,
                            source: element.source.clone(),
                        })
                    }
                    "group" => {
                        attrs(element, &["label"])?;
                        Ok(NavigationItem::Group {
                            label: string(element, "label")?,
                            items: self.navigation_items(&element.children)?,
                            source: element.source.clone(),
                        })
                    }
                    _ => Err(error(
                        element,
                        "KV2001",
                        "navigation 只允许 item 或 group",
                        "将页面布局组件移到 page",
                    )),
                }
            })
            .collect()
    }

    fn check_boundaries(&mut self) {
        let server_roots: BTreeSet<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
            .map(|reference| reference.module.clone())
            .collect();
        let shared_modules: BTreeSet<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::SharedFunction)
            .map(|reference| reference.module.clone())
            .collect();
        let mut private_modules = BTreeSet::new();
        let mut private_queue: VecDeque<_> = self
            .modules
            .iter()
            .filter(|reference| reference.kind == ModuleReferenceKind::ServerFunction)
            .map(|reference| reference.module.clone())
            .chain(
                self.program
                    .modules
                    .iter()
                    .filter(|module| is_server_path(&module.path))
                    .map(|module| module.path.clone()),
            )
            .collect();
        while let Some(path) = private_queue.pop_front() {
            if !server_roots.contains(&path)
                && !is_server_path(&path)
                && (shared_modules.contains(&path)
                    || path
                        .to_string_lossy()
                        .split(['/', '\\'])
                        .any(|part| part.contains(".shared.")))
            {
                continue;
            }
            if !private_modules.insert(path.clone()) {
                continue;
            }
            for edge in self.program.module_graph.edges.iter().filter(|edge| {
                edge.importer == path
                    && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
            }) {
                if let Some(ResolvedModule::File { path }) = &edge.target {
                    private_queue.push_back(path.clone());
                }
            }
        }
        for reference in &self.modules {
            if !matches!(
                reference.kind,
                ModuleReferenceKind::ClientComponent
                    | ModuleReferenceKind::ClientFunction
                    | ModuleReferenceKind::ClientExpression
                    | ModuleReferenceKind::SharedFunction
            ) {
                continue;
            }
            if reference.external {
                if is_server_path(&reference.module)
                    || reference.module.to_string_lossy().starts_with("node:")
                {
                    self.diagnostics.push(boundary(
                        reference.source.clone(),
                        "客户端不允许导入 Node 或服务端包路径",
                    ));
                }
                continue;
            }
            let sliced = reference.kind == ModuleReferenceKind::ClientExpression
                || reference.module == self.entry_source.path
                || reference.module.to_string_lossy().contains(".diu.");
            if server_roots.contains(&reference.module) {
                self.diagnostics.push(boundary(
                    reference.source.clone(),
                    "同一个模块不能同时注册服务端私有函数和浏览器或 Shared 函数",
                ));
                continue;
            }
            let mut queue = if sliced {
                match crate::javascript::source_slice::slice(self.program, reference) {
                    Ok(slice) => {
                        let mut queue = VecDeque::new();
                        for target in slice.imports.into_values() {
                            match target {
                                crate::javascript::source_slice::ClientImportTarget::File(path) => {
                                    queue.push_back(path)
                                }
                                crate::javascript::source_slice::ClientImportTarget::External(
                                    name,
                                ) if name.starts_with("node:")
                                    || is_server_path(Path::new(&name)) =>
                                {
                                    self.diagnostics.push(boundary(
                                        reference.source.clone(),
                                        "客户端切片依赖包含服务端模块",
                                    ))
                                }
                                _ => {}
                            }
                        }
                        queue
                    }
                    Err(error) => {
                        self.diagnostics
                            .push(boundary(reference.source.clone(), error.message));
                        continue;
                    }
                }
            } else {
                VecDeque::from([reference.module.clone()])
            };
            let mut visited = BTreeSet::new();
            while let Some(path) = queue.pop_front() {
                if !visited.insert(path.clone()) {
                    continue;
                }
                if private_modules.contains(&path) || is_server_path(&path) {
                    let mut failure = boundary(
                        reference.source.clone(),
                        format!("客户端引用可到达服务端私有模块 {}", path.display()),
                    );
                    if let Some(module) = self
                        .program
                        .modules
                        .iter()
                        .find(|module| module.path == path)
                        && let Some(export) = module.exports.first()
                    {
                        failure.related.push(SourceLocation {
                            path: path.clone(),
                            span: export.span.clone(),
                        });
                    }
                    self.diagnostics.push(failure);
                    break;
                }
                if let Some(module) = self
                    .program
                    .modules
                    .iter()
                    .find(|module| module.path == path)
                {
                    // 动态 import 和 require 尚无可证明安全的闭包，禁止它们绕过静态图。
                    if module.expressions.iter().any(|expr| {
                        module.raw(&expr.span).is_some_and(|raw| {
                            (expr.kind == "Call" && dynamic_import(raw))
                                || raw.trim_start().starts_with("import.meta")
                        })
                    }) || module.semantic.references.iter().any(|reference| {
                        reference.name == "require"
                            && reference.namespace == kengine::SymbolNamespace::Value
                            && reference.symbol.is_none()
                    }) {
                        self.diagnostics.push(boundary(
                            reference.source.clone(),
                            "客户端模块含未建模的动态模块加载",
                        ));
                        break;
                    }
                }
                for edge in self.program.module_graph.edges.iter().filter(|edge| {
                    edge.importer == path
                        && matches!(edge.kind, DependencyKind::Import | DependencyKind::Reexport)
                }) {
                    match &edge.target {
                        Some(ResolvedModule::File { path }) => queue.push_back(path.clone()),
                        Some(ResolvedModule::External { specifier })
                            if specifier.starts_with("node:")
                                || is_server_path(Path::new(specifier)) =>
                        {
                            self.diagnostics.push(boundary(
                                reference.source.clone(),
                                "客户端依赖不能包含 Node 或服务端私有模块",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn source_of(node: &StaticNode) -> SourceLocation {
    match node {
        StaticNode::Element(element) => element.source.clone(),
        StaticNode::Text { source, .. } => source.clone(),
    }
}

fn start_of_file() -> kengine::SourceSpan {
    kengine::SourceSpan {
        start: 0,
        end: 0,
        start_position: kengine::Position { line: 1, column: 1 },
        end_position: kengine::Position { line: 1, column: 1 },
    }
}

fn boundary(source: SourceLocation, message: impl Into<String>) -> DslDiagnostic {
    let mut error = DslDiagnostic::new(
        "KV2101",
        source,
        message,
        "拆分独立的 client/shared/server 模块，并只注册允许进入浏览器的导出",
    );
    error.category = DslDiagnosticCategory::Boundary;
    error
}

fn is_server_path(path: &Path) -> bool {
    path.to_string_lossy()
        .split(['/', '\\'])
        .any(|part| part.contains(".server."))
}

fn dynamic_import(raw: &str) -> bool {
    raw.trim_start()
        .strip_prefix("import")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '/'))
}

fn function_expression(module: &kengine::ModuleIR, mut id: kengine::ExpressionId) -> bool {
    let mut seen = BTreeSet::new();
    while seen.insert(id) {
        let Some(expression) = module.expression(id) else {
            return false;
        };
        if matches!(expression.kind.as_str(), "ArrowFunction" | "Function") {
            return true;
        }
        if let kengine::ExpressionShape::Wrapper { expression, .. } = expression.shape {
            id = expression;
        } else {
            return false;
        }
    }
    false
}

fn action_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn check_projection(
    element: &StaticElement,
    resource: &ResourceDefinition,
    fields: &[String],
) -> Result<()> {
    if let Some(name) = fields.iter().find(|name| resource.field(name).is_none()) {
        return Err(error(
            element,
            "KV2011",
            format!("Resource {} 没有字段 {name}", resource.name),
            "投影只允许已声明的业务字段",
        ));
    }
    Ok(())
}

fn collect_page_fields(nodes: &[StaticNode], fields: &mut Vec<FieldDefinition>) -> Result<()> {
    for node in nodes {
        if let StaticNode::Element(element) = node
            && element.module_ref.is_none()
        {
            if field_kind(&element.name).is_some() {
                fields.push(field(element)?);
            } else if element.name == "group" {
                attrs(element, &["label"])?;
                collect_page_fields(&element.children, fields)?;
            }
        }
    }
    Ok(())
}

fn has_control(nodes: &[StaticNode]) -> bool {
    nodes.iter().any(|node| matches!(node, StaticNode::Element(element) if element.module_ref.is_none() && (matches!(element.name.as_str(), "form" | "table") || (element.name == "group" && has_control(&element.children)))))
}

struct PreparedPage<'a> {
    element: &'a StaticElement,
    path: String,
    resource: Option<String>,
    fields: Vec<FieldDefinition>,
}
struct PageState<'a> {
    path: &'a str,
    permission: Role,
    auto_resource: Option<&'a str>,
    auto_fields: &'a [FieldDefinition],
    inserted_form: bool,
    form_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kengine::{AnalyzeOptions, Engine};
    use std::{
        error::Error,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    type TestResult<T = ()> = std::result::Result<T, Box<dyn Error>>;
    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new(app: &str, files: &[(&str, &str)]) -> TestResult<Self> {
            let root = std::env::temp_dir().join(format!(
                "kenvoy-dsl-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
            fs::create_dir(&root)?;
            fs::write(root.join("app.tsx"), app)?;
            for (name, source) in files {
                fs::write(root.join(name), source)?;
            }
            Ok(Self { root })
        }
        fn analyze(&self) -> TestResult<AnalyzeResult> {
            let program = Engine::new().analyze_project(
                &self.root,
                AnalyzeOptions {
                    entry_points: vec!["app.tsx".into()],
                    ..Default::default()
                },
            )?;
            assert!(!program.has_errors(), "{:?}", program.diagnostics);
            Ok(analyze(
                &program,
                &self.root.join("app.tsx").canonicalize()?,
                "测试应用",
            ))
        }
        fn valid(&self) -> TestResult<ApplicationIR> {
            let result = self.analyze()?;
            assert!(!result.has_errors(), "{:?}", result.diagnostics);
            Ok(result.application.ok_or("缺少 Application IR")?)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.root.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }

    #[test]
    fn files_and_images_produce_complete_ir_without_changing_scalar_serialization() -> TestResult {
        let application = Fixture::new(
            r#"import {View} from './view'; export default <app name='附件'><override name='file' component={View}/><override name='image' component={View}/><resource name='items'><text name='title'/><file name='attachments' label='附件' maxFiles={20} maxSize={1} accept={['image/*','application/pdf']} default={[]}/><file name='other' default={null}/></resource><page path='/items'><form resource='items'/><group label='图片'><image src='https://example.com/a.png' alt='' width={320} height={4096} fit='cover'/><image src='/images/a.png' alt='示例'/></group></page></app>;"#,
            &[("view.tsx", "export function View(){return <div/>;}")],
        )?.valid()?;
        let fields = &application.resources[0].fields;
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1].kind, FieldKind::File);
        assert_eq!(
            fields[1].upload,
            Some(FileConstraints {
                accept: vec!["image/*".into(), "application/pdf".into()],
                max_size: 1,
                max_files: 20,
            })
        );
        assert_eq!(fields[1].default, Some(serde_json::json!([])));
        assert_eq!(fields[2].upload, Some(FileConstraints::default()));
        assert_eq!(fields[2].default, Some(Value::Null));
        assert!(
            !serde_json::to_value(&fields[0])?
                .as_object()
                .ok_or("缺少字段对象")?
                .contains_key("upload")
        );
        assert_eq!(application.component_overrides.len(), 2);
        let ComponentDefinition::Group { children, .. } = &application.pages[0].components[1]
        else {
            return Err("缺少图片组".into());
        };
        assert!(matches!(&children[0], ComponentDefinition::Image {
            src, alt, width: Some(320), height: Some(4096), fit, ..
        } if src == "https://example.com/a.png" && alt.is_empty() && fit == "cover"));
        assert!(matches!(&children[1], ComponentDefinition::Image {
            width: None, height: None, fit, ..
        } if fit == "contain"));
        let restored: ApplicationIR = serde_json::from_slice(&serde_json::to_vec(&application)?)?;
        assert_eq!(restored, application);
        let implicit = Fixture::new(
            "export default <page path='/items'><group><file name='attachments'/><image src='/a.png' alt='图片'/></group></page>;",
            &[],
        )?.valid()?;
        assert_eq!(implicit.resources[0].fields.len(), 1);
        assert_eq!(implicit.resources[0].fields[0].kind, FieldKind::File);
        Ok(())
    }

    #[test]
    fn file_declarations_reject_invalid_constraints_defaults_and_children() -> TestResult {
        for declaration in [
            "<file name='attachments' maxFiles={0}/>",
            "<file name='attachments' maxFiles={21}/>",
            "<file name='attachments' maxFiles={1.5}/>",
            "<file name='attachments' maxFiles='2'/>",
            "<file name='attachments' maxSize={0}/>",
            "<file name='attachments' maxSize={10485761}/>",
            "<file name='attachments' accept='image/png'/>",
            "<file name='attachments' accept={['.png']}/>",
            "<file name='attachments' accept={['Image/png']}/>",
            "<file name='attachments' accept={['*/*']}/>",
            "<file name='attachments' accept={['image/png','image/png']}/>",
            "<file name='attachments' default={[{}]}/>",
            "<file name='attachments' default='[]'/>",
            "<file name='attachments' required default={[]}/>",
            "<file name='attachments' required default={null}/>",
            "<file name='attachments' unique/>",
            "<file name='attachments' index={false}/>",
            "<file name='attachments' min={0}/>",
            "<file name='attachments' maxLength={1}/>",
            "<file name='attachments'><image src='/a.png' alt=''/></file>",
            "<image src='/a.png' alt=''/>",
        ] {
            let source = format!(
                "export default <app name='非法附件'><resource name='items'>{declaration}</resource></app>;"
            );
            let result = Fixture::new(&source, &[])?.analyze()?;
            assert!(result.application.is_none(), "{declaration}");
            assert!(!result.diagnostics.is_empty(), "{declaration}");
        }
        Ok(())
    }

    #[test]
    fn images_reject_invalid_attributes_urls_and_business_children() -> TestResult {
        for declaration in [
            "<image src='/a.png'/>",
            "<image src='/a.png' alt={null}/>",
            "<image src='/a.png' alt='' width={0}/>",
            "<image src='/a.png' alt='' height={4097}/>",
            "<image src='/a.png' alt='' width={2.5}/>",
            "<image src='/a.png' alt='' fit='fill'/>",
            "<image src='//example.com/a.png' alt=''/>",
            "<image src='https://user:secret@example.com/a.png' alt=''/>",
            "<image src='javascript:alert(1)' alt=''/>",
            "<image src='data:image/png;base64,AA' alt=''/>",
            "<image src='blob:https://example.com/id' alt=''/>",
            "<image src='/a.png' alt=''><file name='attachments'/></image>",
        ] {
            let source = format!("export default <page path='/view'>{declaration}</page>;");
            let result = Fixture::new(&source, &[])?.analyze()?;
            assert!(result.application.is_none(), "{declaration}");
            assert!(!result.diagnostics.is_empty(), "{declaration}");
        }
        Ok(())
    }

    #[test]
    fn option_dependencies_never_treat_file_arrays_as_scalars() -> TestResult {
        for selector in [
            "<select name='choice'><option value='a' when={{attachments:null}}/></select>",
            "<select name='choice'><option value='a' when={{attachments:[]}}/></select>",
            "<select name='choice' optionsSource={{resource:'sources',value:'attachments',label:'title',permission:'anonymous'}}/>",
            "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'attachments',permission:'anonymous'}}/>",
            "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{attachments:'attachments'},permission:'anonymous'}}/>",
            "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{title:'attachments'},permission:'anonymous'}}/>",
            "<select name='choice' optionsSource={{resource:'sources',value:'id',label:'title',filters:{attachments:'title'},permission:'anonymous'}}/>",
        ] {
            let source = format!(
                "export default <app name='联动'><resource name='sources'><text name='title'/><file name='attachments'/></resource><resource name='items'><text name='title'/><file name='attachments'/>{selector}</resource></app>;"
            );
            let result = Fixture::new(&source, &[])?.analyze()?;
            assert!(result.application.is_none(), "{selector}");
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "KV2015"),
                "{selector}: {:?}",
                result.diagnostics
            );
        }
        Ok(())
    }

    #[test]
    fn linked_options_sources_and_overrides_are_explicit_and_validated() -> TestResult {
        let source = r#"import {SelectView} from './view'; export default <app name='选项'><override name='select' component={SelectView}/><resource name='cities'><text name='code' unique/><text name='name'/><text name='province'/></resource><resource name='customers'><select name='province'><option value='zj'/><option value='js'/></select><select name='city' optionsSource={{resource:'cities',value:'code',label:'name',filters:{province:'province'},permission:'anonymous'}}/><select name='district'><option value='xihu' when={{province:'zj'}}/><option value='gulou' when={{province:'js'}}/></select></resource><page path='/customers'><form resource='customers'/></page></app>;"#;
        let files = [(
            "view.tsx",
            "export function SelectView(props: unknown){return <select/>;}",
        )];
        let app = Fixture::new(source, &files)?.valid()?;
        assert_eq!(app.component_overrides.len(), 1);
        assert_eq!(
            app.module_references[0].kind,
            ModuleReferenceKind::ClientComponent
        );
        let city = app
            .resource("customers")
            .ok_or("缺少资源")?
            .field("city")
            .ok_or("缺少字段")?;
        assert_eq!(
            city.options_source.as_ref().ok_or("缺少来源")?.resource,
            "cities"
        );
        for invalid in [
            source.replace("value:'code'", "value:'name'"),
            source.replace("permission:'anonymous'", "permission:'root'"),
            source.replace("permission:'anonymous'", "permission:'admin'"),
            source.replace(
                "filters:{province:'province'}",
                "filters:{province:'missing'}",
            ),
            source.replace("when={{province:'zj'}}", "when={{district:'xihu'}}"),
            source.replace("component={SelectView}", "component={'SelectView'}"),
            source.replace("name='select' component", "name='unknown' component"),
            source.replace(
                "<resource name='cities'>",
                "<override name='select' component={SelectView}/><resource name='cities'>",
            ),
            source.replace(
                "<form resource='customers'/>",
                "<form resource='customers' fields={['city']}/>",
            ),
        ] {
            assert!(
                Fixture::new(&invalid, &files)?
                    .analyze()?
                    .application
                    .is_none(),
                "应拒绝 {invalid}"
            );
        }
        let mut old = serde_json::to_value(&app)?;
        old.as_object_mut()
            .ok_or("缺少对象")?
            .remove("component_overrides");
        let restored: ApplicationIR = serde_json::from_value(old)?;
        assert!(restored.component_overrides.is_empty());
        Ok(())
    }

    #[test]
    fn customer_acceptance_builds_one_model_with_controlled_public_create() -> TestResult {
        let fixture = Fixture::new(
            r#"export default <app name="客户管理系统"><resource name="customers"><text name="name" label="客户姓名" required maxLength={100}/><text name="phone" label="联系电话"/><number name="age" label="年龄" integer min={0}/></resource><page path="/customers" title="客户登记"><form resource="customers" mode="create"/></page></app>;"#,
            &[],
        )?;
        let application = fixture.valid()?;
        assert_eq!(application.name, "客户管理系统");
        assert_eq!(application.resources.len(), 1);
        assert_eq!(application.pages.len(), 1);
        let resource = application.resource("customers").ok_or("客户资源缺失")?;
        assert_eq!(resource.fields.len(), 3);
        assert_eq!(
            resource.field("name").and_then(|field| field.max_length),
            Some(100)
        );
        let public: Vec<_> = application
            .permissions
            .iter()
            .filter(|permission| permission.role == Role::Anonymous)
            .collect();
        assert_eq!(public.len(), 1);
        assert_eq!(public[0].operation, Operation::Create);
        assert_eq!(public[0].fields, ["name", "phone", "age"]);
        assert_eq!(
            application
                .permissions
                .iter()
                .filter(|permission| permission.role == Role::Admin)
                .count(),
            5
        );
        let encoded = serde_json::to_value(&application)?;
        assert_eq!(encoded["resources"][0]["fields"][0]["kind"], "text");
        assert_eq!(
            serde_json::from_value::<ApplicationIR>(encoded)?,
            application
        );
        Ok(())
    }

    #[test]
    fn all_field_kinds_validate_constraints_options_and_defaults() -> TestResult {
        let application = Fixture::new(r#"export default <app name="字段"><resource name="items"><text name="name" required default="张三" maxLength={4} unique/><textarea name="note" default={null}/><number name="amount" min={0} max={100} integer default={10}/><boolean name="enabled" default={true}/><date name="day" default="2024-02-29"/><datetime name="time" default="2024-02-29T08:00:00+08:00"/><select name="status" default="new" index><option value="new" label="新增"/><option value="done"/></select></resource></app>;"#, &[])?.valid()?;
        let fields = &application.resources[0].fields;
        assert_eq!(fields.len(), 7);
        assert_eq!(fields[1].default, Some(Value::Null));
        assert_eq!(
            fields[5].default,
            Some(Value::String("2024-02-29T00:00:00.000Z".into()))
        );
        let round_trip: ApplicationIR = serde_json::from_slice(&serde_json::to_vec(&application)?)?;
        assert_eq!(round_trip, application);
        assert_eq!(round_trip.resources[0].fields[1].default, Some(Value::Null));
        Ok(())
    }

    #[test]
    fn select_options_reject_nul_without_requiring_a_default_value() -> TestResult {
        for attribute in [r#"{"a\u0000b"}"#, r#"{"\0"}"#] {
            let source = format!(
                "export default <app name='选项验证'><resource name='items'><select name='status'><option value={attribute}/></select></resource></app>;"
            );
            let result = Fixture::new(&source, &[])?.analyze()?;
            assert!(result.application.is_none());
            assert!(result.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "KV2007" && diagnostic.message.contains("NUL")
            }));
        }
        Ok(())
    }

    #[test]
    fn implicit_resources_resolve_after_all_pages_and_reuse_explicit_schema() -> TestResult {
        let app = r#"export default <app name="复用"><page path="/list"><table resource="customers"/></page><page path="/customers"><text name="name" required/></page><page path="/customers/new" resource="customers"><text name="name" label="名称" required/></page></app>;"#;
        let application = Fixture::new(app, &[])?.valid()?;
        assert_eq!(application.resources.len(), 1);
        assert_eq!(application.pages.len(), 3);
        let ComponentDefinition::Form { labels: first, .. } = &application.pages[1].components[0]
        else {
            return Err("缺少页面简写表单".into());
        };
        let ComponentDefinition::Form { labels: second, .. } = &application.pages[2].components[0]
        else {
            return Err("缺少页面简写表单".into());
        };
        assert_eq!(first.get("name").map(String::as_str), Some("name"));
        assert_eq!(second.get("name").map(String::as_str), Some("名称"));
        assert_eq!(application.resources[0].fields[0].label, "name");
        assert!(
            !application
                .permissions
                .iter()
                .any(|permission| permission.role == Role::Anonymous
                    && permission.operation == Operation::List)
        );
        let explicit = r#"export default <app name="显式"><page path="/customers"><text name="name" required/></page><resource name="customers"><text name="name" label="显式名称" required/></resource></app>;"#;
        let application = Fixture::new(explicit, &[])?.valid()?;
        assert_eq!(application.resources[0].fields[0].label, "显式名称");
        Ok(())
    }

    #[test]
    fn schema_components_expand_but_ui_function_jsx_never_becomes_schema() -> TestResult {
        let fixture = Fixture::new(
            r#"import Fields from './fields.diu'; import {Card} from './card'; export default <app name="组件"><resource name="customers"><Fields/></resource><page path="/customers"><Card title="客户"/><form resource="customers"/></page></app>;"#,
            &[
                (
                    "fields.diu.tsx",
                    "export default <><text name='name' required/><number name='age' integer min={0}/></>;",
                ),
                (
                    "card.tsx",
                    "export function Card(){return <text name='not_a_field'/>;}",
                ),
            ],
        )?;
        let application = fixture.valid()?;
        assert_eq!(application.resources[0].fields.len(), 2);
        assert!(application.resources[0].field("not_a_field").is_none());
        assert_eq!(application.module_references.len(), 1);
        assert_eq!(
            application.module_references[0].kind,
            ModuleReferenceKind::ClientComponent
        );
        assert!(matches!(
            application.pages[0].components[0],
            ComponentDefinition::Custom { .. }
        ));
        Ok(())
    }

    #[test]
    fn actions_are_explicit_and_navigation_inherits_target_permission() -> TestResult {
        let fixture = Fixture::new(
            r#"import {calculateDiscount} from './discount.server'; export default <app name="动作"><action name="calculateDiscount" handler={calculateDiscount} runtime="server" permission="admin"/><page path="/private" permission="admin"><group label="内容"/></page><navigation label="导航"><group label="管理"><item label="私有" path="/private"/></group></navigation></app>;"#,
            &[(
                "discount.server.ts",
                "export function calculateDiscount(amount: number) { return amount >= 10000 ? 0.8 : 0.9; }",
            )],
        )?;
        let application = fixture.valid()?;
        assert_eq!(application.actions[0].permission, Role::Admin);
        assert_eq!(
            application.module_references[0].kind,
            ModuleReferenceKind::ServerFunction
        );
        let NavigationItem::Group { items, .. } = &application.navigation[0].items[0] else {
            return Err("导航组缺失".into());
        };
        assert!(matches!(
            items[0],
            NavigationItem::Link {
                permission: Role::Admin,
                ..
            }
        ));
        let arrow = Fixture::new(r#"import {compute} from './compute'; export default <app name="箭头"><action name="compute" handler={compute} runtime="server" permission="user"/></app>;"#,
            &[("compute.ts", "export const compute = (value: number) => value * 2;")])?.valid()?;
        assert_eq!(arrow.actions.len(), 1);
        Ok(())
    }

    #[test]
    fn semantic_failures_never_publish_a_partial_application() -> TestResult {
        let cases = [
            (
                r#"export default <app name="x"><resource name="items"><text name="same"/><text name="same"/></resource></app>;"#,
                "KV2005",
            ),
            (
                r#"export default <page path="/items"><number name="age" min={10} max={0}/></page>;"#,
                "KV2007",
            ),
            (
                r#"export default <page path="/items"><number name="age" integer default={1.2}/></page>;"#,
                "KV2008",
            ),
            (
                r#"export default <page path="/items"><number name="age" maxLength={3}/></page>;"#,
                "KV2002",
            ),
            (
                r#"export default <page path="/items"><text name="name" required="yes"/></page>;"#,
                "KV2004",
            ),
            (
                r#"export default <page path="/items/new"><text name="name"/></page>;"#,
                "KV2007",
            ),
            (
                r#"export default <page path="/items"><form resource="missing"/></page>;"#,
                "KV2011",
            ),
            (
                r#"export default <app name="x"><page path="/items"/><page path="/items"/></app>;"#,
                "KV2009",
            ),
            (
                r#"export default <app name="x"><resource name="items"><text name="name"/></resource><page path="/items"><number name="name"/></page></app>;"#,
                "KV2010",
            ),
            (
                r#"export default <><page path="/one"/><page path="/two"/></>;"#,
                "KV2001",
            ),
            (r#"export default <></>;"#, "KV2001"),
        ];
        for (source, expected) in cases {
            let result = Fixture::new(source, &[])?.analyze()?;
            assert!(result.application.is_none(), "应阻断：{source}");
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected),
                "期望 {expected}，得到 {:?}",
                result.diagnostics
            );
        }
        Ok(())
    }

    #[test]
    fn client_reexports_and_dynamic_import_cannot_reach_server_modules() -> TestResult {
        for body in [
            "import {secret} from './secret.server'; export function Card(){return secret;}",
            "export {secret as Card} from './secret.server';",
            "export function Card(){return import /* hidden */ ('./secret.server');}",
        ] {
            let fixture = Fixture::new(
                "import {Card} from './card'; export default <page path='/view'><Card/></page>;",
                &[
                    ("card.tsx", body),
                    (
                        "secret.server.ts",
                        "export function secret(){return 'PRIVATE';}",
                    ),
                ],
            )?;
            let result = fixture.analyze()?;
            assert!(result.application.is_none());
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "KV2101"),
                "{:?}",
                result.diagnostics
            );
        }
        let safe = Fixture::new("import {Card} from './card'; export default <page path='/view'><Card/></page>;", &[("card.tsx", "import type {Secret} from './secret.server'; export function Card(){const value: Secret | null = null; return value;}"), ("secret.server.ts", "export interface Secret { value: string }")])?.valid()?;
        assert_eq!(safe.module_references.len(), 1);
        Ok(())
    }

    #[test]
    fn form_field_policies_remain_separate_and_invalid_versions_are_rejected() -> TestResult {
        let application = Fixture::new(r#"export default <app name="表单"><resource name="items"><text name="name" required/><text name="phone"/><text name="note"/></resource><page path="/items"><form resource="items" fields={['name','phone']}/><form resource="items" fields={['name','note']}/></page></app>;"#, &[])?.valid()?;
        let policies: Vec<_> = application
            .permissions
            .iter()
            .filter(|policy| policy.role == Role::Anonymous)
            .collect();
        assert_eq!(policies.len(), 2);
        assert_eq!(policies[0].fields, ["name", "phone"]);
        assert_eq!(policies[1].fields, ["name", "note"]);
        assert_ne!(policies[0].form, policies[1].form);
        let mut value = serde_json::to_value(application)?;
        value["format_version"] = Value::from(99);
        assert!(serde_json::from_value::<ApplicationIR>(value).is_err());
        Ok(())
    }

    #[test]
    fn server_private_dependency_closure_requires_explicit_shared_boundaries() -> TestResult {
        let app = "import {Card} from './card'; import {execute} from './logic.server'; export default <app name='隔离'><action name='execute' handler={execute} runtime='server' permission='admin'/><page path='/view'><Card/></page></app>;";
        let leaked = Fixture::new(
            app,
            &[
                (
                    "logic.server.ts",
                    "import {secret} from './helper'; export function execute(){return secret;}",
                ),
                ("helper.ts", "export const secret = 'PRIVATE';"),
                (
                    "card.tsx",
                    "import {secret} from './helper'; export function Card(){return secret;}",
                ),
            ],
        )?
        .analyze()?;
        assert!(leaked.application.is_none());
        assert!(
            leaked
                .diagnostics
                .iter()
                .any(|error| error.code == "KV2101")
        );
        let shared = Fixture::new(app, &[("logic.server.ts", "import {format} from './helper.shared'; export function execute(){return format(1);}"), ("helper.shared.ts", "export const format = (value: number) => String(value);"), ("card.tsx", "import {format} from './helper.shared'; export function Card(){return format(2);}")])?.valid()?;
        assert_eq!(shared.actions.len(), 1);
        Ok(())
    }

    #[test]
    fn bundler_globs_and_aliased_require_are_not_unchecked_module_edges() -> TestResult {
        for body in [
            "export function Card(){return import.meta.glob('./*.server.ts', {eager:true});}",
            "const load = require; export function Card(){return load('./secret.server');}",
        ] {
            let result = Fixture::new(
                "import {Card} from './card'; export default <page path='/view'><Card/></page>;",
                &[("card.tsx", body)],
            )?
            .analyze()?;
            assert!(result.application.is_none());
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|error| error.code == "KV2101"),
                "{:?}",
                result.diagnostics
            );
        }
        Ok(())
    }
}
