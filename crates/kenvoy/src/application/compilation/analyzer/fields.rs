use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    model::{
        FieldDefinition, FieldKind, FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE,
        OptionsSource, SelectOption,
    },
    source::SourceLocation,
    validation,
};

use super::super::static_eval::{StaticElement, StaticNode};
use super::{
    Result,
    attributes::{
        attrs, boolean, elements, error, identifier, intrinsic, json, list, no_children, number,
        positive_integer, required_string, semantic_error, string,
    },
};

pub(super) fn field_kind(name: &str) -> Option<FieldKind> {
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

pub(super) fn field(element: &StaticElement) -> Result<FieldDefinition> {
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
            error(
                element,
                "KV2007",
                message.message(),
                "检查文件数量、大小与 MIME 限制",
            )
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
        options_source: json(element, "optionsSource")?.map(|value| serde_json::from_value::<OptionsSource>(value.clone()).map_err(|_| error(element,"KV2015","optionsSource 必须声明 resource/value/label/permission，可选 filters 为源字段到本资源字段的映射","使用静态对象；permission 显式取 anonymous/user/admin"))).transpose()?,
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

pub(super) fn unique_fields(fields: &[FieldDefinition]) -> Result<()> {
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

pub(super) fn collect_page_fields(
    nodes: &[StaticNode],
    fields: &mut Vec<FieldDefinition>,
) -> Result<()> {
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

pub(super) fn has_control(nodes: &[StaticNode]) -> bool {
    nodes.iter().any(|node| matches!(node, StaticNode::Element(element) if element.module_ref.is_none() && (matches!(element.name.as_str(), "form" | "table") || (element.name == "group" && has_control(&element.children)))))
}
