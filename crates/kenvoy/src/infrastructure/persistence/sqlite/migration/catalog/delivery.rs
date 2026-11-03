//! 数据库交付前缀与无源码审计核验。
use super::super::super::schema::{SchemaModel, hash, schema_hash};
use super::super::{
    MigrationError, Result,
    state::{self, State},
};
use super::{
    Catalog,
    model::require,
    records::{Records, read, verify_owner},
};
use rusqlite::Connection;
pub(super) fn prefix(
    connection: &Connection,
    catalog: &Catalog,
    state: &State,
) -> Result<(Records, usize)> {
    let records = read(connection)?;
    let mut position = 0;
    if let Some(baseline) = &records.baseline {
        require(
            baseline.ordinal <= catalog.steps.len(),
            "数据库 baseline 领先于当前交付链，禁止降级",
        )?;
        for (index, checksum) in baseline.skipped_checksums.iter().enumerate() {
            require(
                &catalog.steps[index].checksum == checksum,
                "baseline 跳过的历史已被改写或分叉",
            )?;
        }
        require(
            catalog.steps[baseline.ordinal - 1].id == baseline.at_id,
            "baseline 位置与交付 ID 不一致",
        )?;
        let legacy = state::history(connection)?;
        require(
            legacy.len() >= baseline.legacy_history.len()
                && hash(&legacy[..baseline.legacy_history.len()])?
                    == hash(&baseline.legacy_history)?,
            "baseline 原始旧迁移历史被改写",
        )?;
        position = baseline.ordinal;
    }
    let legacy = state::history(connection)?;
    let legacy_start = records
        .baseline
        .as_ref()
        .map_or(0, |baseline| baseline.legacy_history.len());
    for (index, record) in records.applied.iter().enumerate() {
        require(
            position < catalog.steps.len(),
            "数据库已执行的迁移领先于当前交付链，禁止降级",
        )?;
        let step = &catalog.steps[position];
        require(
            record.ordinal == position + 1
                && record.id == step.id
                && record.checksum == step.checksum
                && record.from_hash == step.from_hash
                && record.to_hash == step.to_hash
                && record.legacy_version == (legacy_start + index + 1) as u64,
            "数据库成功记录不是当前交付链的有效前缀",
        )?;
        require(
            legacy.get(legacy_start + index).is_some_and(|old| {
                old.version == record.legacy_version
                    && old.from_hash == record.from_hash
                    && old.to_hash == record.to_hash
            }),
            "正式执行记录与真实旧历史不一致",
        )?;
        position += 1;
    }
    if records.baseline.is_none()
        && records.applied.is_empty()
        && (state.version > 0 || !state.model.tables.is_empty())
    {
        return Err(MigrationError::Drift(
            "旧数据库没有正式交付链，必须显式 migrate baseline 接入，不能当作空库".into(),
        ));
    }
    require(
        legacy.len() == legacy_start + records.applied.len(),
        "交付记录之外存在旧动态迁移执行，必须显式处理历史分叉",
    )?;
    let expected = position
        .checked_sub(1)
        .map(|index| &catalog.steps[index].schema)
        .cloned()
        .unwrap_or_default();
    require(
        expected == state.model,
        "数据库 Schema 与正式交付位置不一致",
    )?;
    Ok((records, position))
}
pub(super) fn audit(connection: &Connection, records: &Records, state: &State) -> Result<()> {
    if !verify_owner(connection)? {
        return Ok(());
    }
    let legacy = state::history(connection)?;
    let start = records
        .baseline
        .as_ref()
        .map_or(0, |baseline| baseline.legacy_history.len());
    let mut ordinal = records
        .baseline
        .as_ref()
        .map_or(0, |baseline| baseline.ordinal);
    let mut previous = records
        .baseline
        .as_ref()
        .map(|baseline| baseline.schema_hash.clone())
        .unwrap_or(schema_hash(&SchemaModel::default())?);
    if let Some(baseline) = &records.baseline {
        require(
            legacy.len() >= start && hash(&legacy[..start])? == hash(&baseline.legacy_history)?,
            "baseline 保存的原始历史已变化",
        )?;
    }
    for (index, record) in records.applied.iter().enumerate() {
        ordinal += 1;
        require(
            record.ordinal == ordinal
                && record.id == format!("{ordinal:06}")
                && record.from_hash == previous
                && record.legacy_version == (start + index + 1) as u64
                && legacy.get(start + index).is_some_and(|old| {
                    old.from_hash == record.from_hash && old.to_hash == record.to_hash
                }),
            "正式执行审计链缺失、重复或与真实旧历史不一致",
        )?;
        previous = record.to_hash.clone();
    }
    require(
        legacy.len() == start + records.applied.len() && previous == schema_hash(&state.model)?,
        "正式审计位置与数据库 Schema 或历史不一致",
    )
}
