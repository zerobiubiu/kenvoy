//! 正式链、兼容性、映射和风险契约校验。
use super::super::super::schema::{SCHEMA_HASH_VERSION, SchemaModel, frozen, schema_hash};
use super::super::{Result, SchemaChange, planning};
use super::{
    Catalog, MigrationStep, Risk, Rules,
    model::{EXECUTOR, FORMAT_VERSION, invalid, require},
    sql::Gate,
};
use rusqlite::Connection;
use std::collections::BTreeSet;

impl Catalog {
    pub fn validate(&self) -> Result<()> {
        require(
            self.format == FORMAT_VERSION && self.schema_hash_version == SCHEMA_HASH_VERSION,
            "不支持的正式迁移格式或 Schema 哈希算法",
        )?;
        let empty = SchemaModel::default();
        let mut previous = &empty;
        let mut previous_id = None;
        for (index, step) in self.steps.iter().enumerate() {
            validate_step(step, previous, previous_id, index + 1, true)?;
            previous = &step.schema;
            previous_id = Some(step.id.as_str());
        }
        scratch(self)
    }
    pub fn validate_target(&self, app: &crate::domain::model::ApplicationIR) -> Result<()> {
        let target = super::super::super::schema::schema_model(app)?;
        let empty = SchemaModel::default();
        let last = self.steps.last().map(|step| &step.schema).unwrap_or(&empty);
        require(
            last == &target,
            "正式迁移链末端与当前应用 Schema 不一致；请先生成并固化迁移",
        )
    }
}

pub(super) fn validate_step(
    step: &MigrationStep,
    previous: &SchemaModel,
    previous_id: Option<&str>,
    ordinal: usize,
    sealed: bool,
) -> Result<()> {
    require(
        step.format == 1
            && step.executor_format == 1
            && step.executor == EXECUTOR
            && step.schema_hash_version == SCHEMA_HASH_VERSION,
        "迁移步骤执行器、格式或哈希算法不兼容",
    )?;
    let required = semver::Version::parse(&step.min_cli)
        .map_err(|_| invalid("迁移最小 CLI 版本不是合法 SemVer"))?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| invalid("CLI 构建版本不是合法 SemVer"))?;
    let minimum = semver::Version::parse(super::model::MIN_CLI_VERSION)
        .map_err(|_| invalid("执行器最低版本无效"))?;
    require(
        required.pre.is_empty()
            && current.cmp_precedence(&required).is_ge()
            && required.cmp_precedence(&minimum).is_ge(),
        "迁移要求不支持的 CLI 版本或预发布契约",
    )?;
    require(
        step.id == format!("{ordinal:06}") && step.previous.as_deref() == previous_id,
        "迁移 ID 必须唯一连续且 previous 必须指向直接前序，禁止缺失、重复或分叉",
    )?;
    require(!step.description.trim().is_empty(), "迁移描述不能为空")?;
    frozen::validate(&step.schema)?;
    require(
        step.from_hash == schema_hash(previous)? && step.to_hash == schema_hash(&step.schema)?,
        "迁移 from/to Schema 哈希不一致",
    )?;
    require(
        step.checksum == step.checksum()?,
        "迁移内容 checksum 不一致，禁止改写历史",
    )?;
    super::rules::validate(previous, &step.schema, &step.rules)?;
    require(
        step.risks == risks(previous, &step.schema, &step.rules)?,
        "迁移风险声明与固定执行语义不一致",
    )?;
    require(
        step.risk_level == super::model::risk_level(&step.risks),
        "迁移风险等级与固定风险声明不一致",
    )?;
    let mut resolved = BTreeSet::new();
    for code in &step.rules.resolved_risks {
        require(
            resolved.insert(code) && step.risks.iter().any(|risk| &risk.code == code),
            "已处理风险代码重复或不存在",
        )?;
    }
    if sealed {
        require(
            step.risks.iter().all(|risk| risk.resolved),
            "迁移包含未解决的风险，禁止固化或执行",
        )?;
    }
    Ok(())
}

pub(super) fn risks(
    previous: &SchemaModel,
    target: &SchemaModel,
    rules: &Rules,
) -> Result<Vec<Risk>> {
    let mut output = vec![];
    for change in planning::diff(previous, target) {
        let (code, description, resolved) = match change {
            SchemaChange::CreateTable { .. }
            | SchemaChange::AddIndex { unique: false, .. }
            | SchemaChange::AddColumn { safe: true, .. } => continue,
            SchemaChange::RenameCandidate { table, from, to } => {
                let resolved = rules
                    .renames
                    .iter()
                    .any(|rule| rule.table == table && rule.from == from && rule.to == to);
                (
                    format!("rename:{table}.{from}->{to}"),
                    "候选改名必须使用明确映射".into(),
                    resolved,
                )
            }
            SchemaChange::DropTable { table } => (
                format!("drop_table:{table}"),
                "删除资源及其数据".into(),
                false,
            ),
            SchemaChange::DropColumn { table, column } => {
                let mapped = rules
                    .renames
                    .iter()
                    .any(|rule| rule.table == table && rule.from == column);
                (
                    format!("drop_column:{table}.{column}"),
                    "删除字段及其数据".into(),
                    mapped,
                )
            }
            SchemaChange::AddColumn { table, column, .. } => {
                let mapped = rules
                    .backfills
                    .iter()
                    .any(|rule| rule.table == table && rule.column == column)
                    || rules
                        .renames
                        .iter()
                        .any(|rule| rule.table == table && rule.to == column);
                (
                    format!("required_column:{table}.{column}"),
                    "无默认值的必填字段需要明确回填".into(),
                    mapped,
                )
            }
            SchemaChange::ChangeColumn { table, column } => {
                let resolved = rules
                    .conversions
                    .iter()
                    .any(|rule| rule.table == table && rule.column == column);
                (
                    format!("change_column:{table}.{column}"),
                    "字段类型或约束变化".into(),
                    resolved,
                )
            }
            SchemaChange::AddIndex { table, index, .. } => (
                format!("unique_index:{table}.{index}"),
                "新增唯一约束".into(),
                true,
            ),
            SchemaChange::DropIndex { table, index } => (
                format!("drop_index:{table}.{index}"),
                "删除索引或唯一约束".into(),
                true,
            ),
        };
        let mandatory = code.starts_with("required_column:") || code.starts_with("rename:");
        let resolved = resolved || (!mandatory && rules.resolved_risks.contains(&code));
        output.push(Risk {
            code,
            description,
            resolved,
        });
    }
    if rules.has_data_operations() {
        output.push(Risk {
            code: "data_sql".into(),
            description: "执行已冻结的数据 SQL、映射或转换".into(),
            resolved: true,
        });
    }
    Ok(output)
}

pub(super) fn scratch(catalog: &Catalog) -> Result<()> {
    scratch_steps(&catalog.steps)
}

pub(super) fn scratch_steps<'a>(steps: impl IntoIterator<Item = &'a MigrationStep>) -> Result<()> {
    let mut connection = Connection::open_in_memory()?;
    let empty = SchemaModel::default();
    let mut previous = &empty;
    for step in steps {
        let gate = Gate::install(&mut connection, super::sql::tables(previous, step))?;
        let transaction = connection.transaction()?;
        super::executor::execute(&transaction, &gate, previous, step)?;
        transaction.commit()?;
        Gate::remove(&mut connection)?;
        previous = &step.schema;
    }
    Ok(())
}
