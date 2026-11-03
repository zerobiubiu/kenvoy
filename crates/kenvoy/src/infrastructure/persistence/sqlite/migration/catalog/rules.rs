//! 显式字段映射、转换和断言的格式 1 契约。
use super::super::super::schema::SchemaModel;
use super::super::Result;
use super::{
    Rules,
    model::{invalid, require},
};
use std::collections::BTreeSet;
pub(super) fn validate(previous: &SchemaModel, target: &SchemaModel, rules: &Rules) -> Result<()> {
    require(rules.format == 1, "不支持的迁移规则格式")?;
    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for rule in &rules.renames {
        let old = previous
            .tables
            .get(&rule.table)
            .ok_or_else(|| invalid("改名映射源表不存在"))?;
        let new = target
            .tables
            .get(&rule.table)
            .ok_or_else(|| invalid("改名映射目标表不存在"))?;
        require(
            old.columns.contains_key(&rule.from)
                && !new.columns.contains_key(&rule.from)
                && new.columns.contains_key(&rule.to)
                && !old.columns.contains_key(&rule.to),
            "改名必须从明确删除的旧列映射到明确新增的目标列",
        )?;
        require(
            sources.insert((&rule.table, &rule.from))
                && destinations.insert((&rule.table, &rule.to)),
            "改名映射重复或分叉",
        )?;
        if old.columns[&rule.from].kind != new.columns[&rule.to].kind {
            require(
                rules
                    .conversions
                    .iter()
                    .any(|r| r.table == rule.table && r.column == rule.to),
                "字段改名并改型必须提供明确转换表达式",
            )?;
        }
    }
    for (label, expressions) in [("回填", &rules.backfills), ("转换", &rules.conversions)] {
        let mut seen = BTreeSet::new();
        for rule in expressions {
            require(
                previous.tables.contains_key(&rule.table)
                    && target
                        .tables
                        .get(&rule.table)
                        .is_some_and(|table| table.columns.contains_key(&rule.column)),
                "回填或转换必须指向现有资源的目标字段",
            )?;
            require(
                !rule.expression.trim().is_empty() && seen.insert((&rule.table, &rule.column)),
                &format!("{label}表达式为空或重复"),
            )?;
        }
    }
    for (table, old) in &previous.tables {
        let Some(new) = target.tables.get(table) else {
            continue;
        };
        for (name, column) in &old.columns {
            if let Some(target) = new.columns.get(name)
                && (column.kind != target.kind || column.integer != target.integer)
            {
                require(
                    rules
                        .conversions
                        .iter()
                        .any(|r| &r.table == table && &r.column == name),
                    "字段改型必须提供明确转换表达式，禁止猜测",
                )?;
            }
        }
    }
    for assertion in rules.preconditions.iter().chain(&rules.assertions).chain(
        rules
            .baseline_proofs
            .iter()
            .flat_map(|proof| &proof.assertions),
    ) {
        require(
            !assertion.sql.trim().is_empty()
                && !assertion.description.trim().is_empty()
                && !assertion.expected.is_array()
                && !assertion.expected.is_object()
                && !assertion.expected.is_boolean(),
            "迁移断言必须声明查询、说明和 SQLite 标量期望",
        )?;
    }
    Ok(())
}
