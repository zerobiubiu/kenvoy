//! 选项元数据的通用业务校验；查询和写入共用声明，显示实现不能改变字段类型。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{Map, Value};

use crate::{
    application::{FieldDefinition, FieldKind, ResourceDefinition, SelectOption},
    dsl::SourceLocation,
    validation::{self, ValidationError, ValidationErrors},
};

pub fn validate_definitions(
    resources: &[ResourceDefinition],
) -> Result<(), (SourceLocation, String)> {
    for resource in resources {
        let fail = |field: &FieldDefinition, message: String| (field.source.clone(), message);
        let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
        for field in &resource.fields {
            if field.kind != FieldKind::Select {
                if field.options_source.is_some() || !field.options.is_empty() {
                    return Err(fail(field, "只有 select 可以声明选项来源".into()));
                }
                continue;
            }
            if field.options_source.is_some() && !field.options.is_empty() {
                return Err(fail(field, "optionsSource 与静态 option 不能混用".into()));
            }
            for option in &field.options {
                if option.when.len() > 16 {
                    return Err(fail(field, "每个 option.when 最多声明 16 个条件".into()));
                }
                for (name, value) in &option.when {
                    let dependent = resource
                        .field(name)
                        .ok_or_else(|| fail(field, format!("选项依赖字段 {name} 不存在")))?;
                    if dependent.kind == FieldKind::File {
                        return Err(fail(
                            field,
                            "file 字段不能作为 option.when 的联动依赖".into(),
                        ));
                    }
                    validation::validate_field(dependent, value)
                        .map_err(|error| fail(field, format!("option.when 条件无效：{error}")))?;
                }
            }
            if let Some(source) = &field.options_source {
                let target = resources
                    .iter()
                    .find(|item| item.name == source.resource)
                    .ok_or_else(|| fail(field, format!("选项资源 {} 不存在", source.resource)))?;
                if source.value != "id"
                    && !target.field(&source.value).is_some_and(|value| {
                        value.unique && matches!(value.kind, FieldKind::Text | FieldKind::Textarea)
                    })
                {
                    return Err(fail(
                        field,
                        "optionsSource.value 必须为 id 或具有 unique 约束的文本字段".into(),
                    ));
                }
                if !target.field(&source.label).is_some_and(|label| {
                    matches!(
                        label.kind,
                        FieldKind::Text | FieldKind::Textarea | FieldKind::Select
                    )
                }) {
                    return Err(fail(
                        field,
                        "optionsSource.label 必须为已声明的文本或 select 字段".into(),
                    ));
                }
                if source.filters.len() > 16 {
                    return Err(fail(
                        field,
                        "optionsSource.filters 最多声明 16 个等值条件".into(),
                    ));
                }
                for (target_name, local_name) in &source.filters {
                    let target_field = target
                        .field(target_name)
                        .ok_or_else(|| fail(field, format!("来源筛选字段 {target_name} 不存在")))?;
                    let local_field = resource
                        .field(local_name)
                        .ok_or_else(|| fail(field, format!("联动字段 {local_name} 不存在")))?;
                    if target_field.kind == FieldKind::File || local_field.kind == FieldKind::File {
                        return Err(fail(
                            field,
                            "file 字段不能作为 optionsSource 的筛选或联动依赖".into(),
                        ));
                    }
                    if category(target_field.kind) != category(local_field.kind) {
                        return Err(fail(
                            field,
                            format!(
                                "联动字段 {local_name} 与来源字段 {target_name} 的数据类型不一致"
                            ),
                        ));
                    }
                }
            }
            dependencies.insert(
                field.name.clone(),
                field
                    .option_dependencies()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            );
        }
        // Kahn 消除法避免对大型声明使用无界递归，同时拒绝自引用和间接循环。
        let mut ready: VecDeque<_> = resource
            .fields
            .iter()
            .filter(|field| dependencies.get(&field.name).is_none_or(BTreeSet::is_empty))
            .map(|field| field.name.clone())
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(name) = ready.pop_front() {
            if !visited.insert(name.clone()) {
                continue;
            }
            for (target, needed) in &mut dependencies {
                if needed.remove(&name) && needed.is_empty() {
                    ready.push_back(target.clone());
                }
            }
        }
        if let Some(field) = resource
            .fields
            .iter()
            .find(|field| !visited.contains(&field.name))
        {
            return Err(fail(field, "选项联动包含自引用或循环依赖".into()));
        }
    }
    Ok(())
}

fn category(kind: FieldKind) -> u8 {
    match kind {
        FieldKind::Number => 1,
        FieldKind::Boolean => 2,
        _ => 0,
    }
}

pub fn matches(
    resource: &ResourceDefinition,
    option: &SelectOption,
    record: &Map<String, Value>,
) -> bool {
    option.when.iter().all(|(name, expected)| {
        let Some(field) = resource.field(name) else {
            return false;
        };
        let Some(actual) = record.get(name) else {
            return false;
        };
        match (
            validation::validate_field(field, actual),
            validation::validate_field(field, expected),
        ) {
            (Ok(actual), Ok(expected)) => actual == expected,
            _ => false,
        }
    })
}

pub fn invalid(field: &FieldDefinition) -> ValidationErrors {
    ValidationErrors(vec![ValidationError {
        code: "KV4013".into(),
        field: Some(field.name.clone()),
        message: "选项不存在、已经失效或不满足当前联动条件".into(),
    }])
}

pub fn validate_static(
    resource: &ResourceDefinition,
    record: &Map<String, Value>,
) -> Result<(), ValidationErrors> {
    for field in &resource.fields {
        let Some(value) = record.get(&field.name).filter(|value| !value.is_null()) else {
            continue;
        };
        if field.kind == FieldKind::Select
            && field.options_source.is_none()
            && !field.options.iter().any(|option| {
                Some(option.value.as_str()) == value.as_str() && matches(resource, option, record)
            })
        {
            return Err(invalid(field));
        }
    }
    Ok(())
}
