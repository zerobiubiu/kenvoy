use std::collections::BTreeSet;

use crate::domain::{
    model::{FieldDefinition, FieldKind},
    validation::{validate_default, validate_file_constraints},
};

use super::super::migration::{MigrationError, Result};

pub(super) fn validate_constraints(field: &FieldDefinition) -> Result<()> {
    match (field.kind, &field.upload) {
        (FieldKind::File, Some(constraints)) => {
            validate_file_constraints(constraints).map_err(|message| {
                MigrationError::InvalidSchema(format!("字段 {}：{message}", field.name))
            })?;
            if field.unique || field.index {
                return Err(MigrationError::InvalidSchema(format!(
                    "file 字段 {} 不支持 unique/index",
                    field.name
                )));
            }
        }
        (FieldKind::File, None) => {
            return Err(MigrationError::InvalidSchema(format!(
                "file 字段 {} 缺少上传约束",
                field.name
            )));
        }
        (_, Some(_)) => {
            return Err(MigrationError::InvalidSchema(format!(
                "非 file 字段 {} 不能包含上传约束",
                field.name
            )));
        }
        (_, None) => {}
    }
    if field.min.is_some_and(|value| !value.is_finite())
        || field.max.is_some_and(|value| !value.is_finite())
        || matches!((field.min,field.max),(Some(min),Some(max)) if min>max)
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 数值范围无效",
            field.name
        )));
    }
    if field.kind != FieldKind::Number
        && (field.integer || field.min.is_some() || field.max.is_some())
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 的数值约束与类型不符",
            field.name
        )));
    }
    if field.integer
        && field
            .min
            .unwrap_or(-crate::domain::validation::MAX_SAFE_INTEGER)
            .max(-crate::domain::validation::MAX_SAFE_INTEGER)
            .ceil()
            > field
                .max
                .unwrap_or(crate::domain::validation::MAX_SAFE_INTEGER)
                .min(crate::domain::validation::MAX_SAFE_INTEGER)
                .floor()
    {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 没有可用的安全整数范围",
            field.name
        )));
    }
    if field.max_length.is_some_and(|limit| limit > 1_000_000) {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 长度限制超出支持范围",
            field.name
        )));
    }
    if !matches!(field.kind, FieldKind::Text | FieldKind::Textarea) && field.max_length.is_some() {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 的长度约束与类型不符",
            field.name
        )));
    }
    if field.kind == FieldKind::Select {
        if field.options.is_empty() && field.options_source.is_none() {
            return Err(MigrationError::InvalidSchema(format!(
                "选择字段 {} 必须有选项",
                field.name
            )));
        }
        let mut names = BTreeSet::new();
        for option in &field.options {
            if !names.insert(&option.value) || option.value.contains('\0') {
                return Err(MigrationError::InvalidSchema(format!(
                    "字段 {} 选项重复或包含 NUL",
                    field.name
                )));
            }
        }
    } else if !field.options.is_empty() {
        return Err(MigrationError::InvalidSchema(format!(
            "字段 {} 非 select 却包含选项",
            field.name
        )));
    }
    if let Some(value) = &field.default {
        validate_default(field, value)
            .map_err(|error| MigrationError::InvalidSchema(error.to_string()))?;
        if value.as_str().is_some_and(|value| value.contains('\0')) {
            return Err(MigrationError::InvalidSchema(
                "SQL 默认文本不能包含 NUL".into(),
            ));
        }
    }
    Ok(())
}
