# 正式迁移与旧库接入操作

状态：参考（现状文档）。迁移归属 Kenvoy CLI，复用 Schema Diff、SQLite 一致性备份与事务执行器，不进入 KEngine。结构与历史设计见 [数据库设计](../architecture/database-design.md)，兼容门禁见 [项目配置](../contracts/project-configuration.md)。

## 生成、审查、固化和交付

```text
kenvoy check
kenvoy migrate generate --description "增加客户电话"
kenvoy migrate seal
kenvoy migrate status
kenvoy build
kenvoy run
```

`create` 为模板生成并固化初始迁移，但不创建数据库。之后修改物理 Schema 必须显式生成新候选、审查并固化；`run`、`build` 和持久化 `dev` 均不会补迁移。单纯 UI、标签或源码位置变化不需要迁移；同 Schema 的数据清洗可提供规则文件生成独立迁移。

正式文件位于项目 `migrations/`：`manifest.json`、追加的步骤文件，以及未固化的 `draft.json`。每步冻结格式、执行器要求、顺序 ID、前序、说明、前后 Schema hash、规范目标 Schema、数据规则、风险等级及 checksum。`risk_level` 为 `safe`、`data` 或 `destructive`，按实际规则重算验证，不能只修改声明降低风险。步骤使用版本化 `sqlite-schema-v1` 声明式执行语义，不调用当前业务函数，不依赖部署源码。构建把完整链封装进 `migrations.json`；无源码环境执行产物中的链。

固化只追加，不改写已有步骤。候选过期、目标 Schema 不一致、未解决的危险项或无法证明的字段改名均拒绝固化；修改规则后重新生成候选。把迁移清单和步骤纳入版本控制；不要修改已经交付、执行的历史。修正必须追加一步。

## 显式规则

```json
{
  "format": 1,
  "renames": [{"table": "customers", "from": "phone", "to": "telephone"}],
  "backfills": [{"table": "customers", "column": "category", "expression": "'普通客户'"}],
  "conversions": [],
  "before_sql": [],
  "after_sql": [],
  "preconditions": [],
  "assertions": [{
    "sql": "SELECT count(*) FROM customers WHERE category IS NULL",
    "expected": 0,
    "description": "客户分类已完成回填"
  }],
  "baseline_proofs": [],
  "resolved_risks": []
}
```

上述文件是规则形状示例，应按真实差异填写，不代表任意应用可直接执行。通过 `migrate generate --description "..." --rules rules.json` 载入；规则冻结进步骤，部署时不再读取原始规则文件。

- `renames` 明确同表字段映射；不根据名称或相似性猜测。
- `backfills` 对新列使用表达式复制，已有列只补 NULL；`conversions` 显式指定目标列转换表达式。SQL 表达式不是 JavaScript，不能调用应用函数。
- 执行顺序：前置断言 → `before_sql` → 固定 Schema 转换与映射复制 → `after_sql` → 目标结构和数据约束核验 → 后置断言。
- 断言是返回单个标量的只读查询，必须等于 `expected`。用它表达需要业务判断的数据语义，不仅验证表结构。
- `resolved_risks` 只承认候选列出的精确风险代码，例如 `drop_column:customers.note`。它不跳过结构、数据或事务校验。
- 自定义 SQL 只获得受限业务数据库权限；禁止控制事务、挂载其他数据库、PRAGMA、访问宿主内部元数据或扩展加载。失败不退回无限制执行。

危险候选按输出的 `confirmation_checksum` 精确固化：

```text
kenvoy migrate seal --confirm <候选完整校验值>
```

固化确认与执行确认不是同一概念：前者确认交付内容，后者绑定当前数据库与待执行计划。不能拿候选 checksum 或全局 yes 代替执行计划 checksum。

## 正式执行与失败恢复

```text
kenvoy migrate status
kenvoy migrate apply --confirm <status 的 confirmation_checksum>
kenvoy run --confirm <status 的 confirmation_checksum>
kenvoy migrate history
```

`status` 只读，不因数据库不存在创建目录或文件。`history` 不依赖源码、迁移文件和项目兼容门禁，输出正式已执行步骤、真实基线来源与保留的旧历史；历史损坏仍明确报错，不伪装为空库。

正式启动先验证配置、产物与链兼容，再获取覆盖迁移和服务运行期的 OS 文件锁。核对实际结构和历史，数据库必须是目标链的有效前缀；领先版本、缺失或改写步骤、重复 ID、分叉、元数据不完整或手工结构漂移均阻断对外服务。业务表没有行不等于空数据库。

- 空数据库从第一步依次执行全部迁移，不直接创建最终结构。
- 每步独立事务，Schema、数据转换、旧结构历史与正式步骤成功记录原子提交；同 Schema 数据迁移也推进正式位置。
- 危险步骤必须已处理风险，并显式确认当前计划，先执行 SQLite backup API 一致性备份。备份失败不执行迁移，确认不能越过数据约束。
- 某步失败只回滚该步；此前提交的步骤保留。重启从真实已提交记录恢复，不按进程日志猜测。
- 全部迁移完成但端口、认证或应用激活失败，报告“数据库已迁移，应用未就绪”；不宣称数据库已回滚，不自动恢复备份。
- 应用就绪/失败另记 `<数据库>.kenvoy-startups.jsonl`，包含应用版本、构建哈希、CLI 版本和当时迁移位置。它是审计记录，不是迁移恢复的事实来源。

## 旧库显式基线接入

旧库没有新迁移文件或正式执行记录，不得当作空库、不得自动重建。先停止旧服务，保留数据库和已有审计历史，再选择可以证明对应关系的正式基线：

```text
kenvoy migrate baseline --at 000001
kenvoy migrate baseline --at 000001 --rules baseline-rules.json
kenvoy migrate baseline --at 000001 --rules baseline-rules.json --confirm <接入报告校验值>
```

无 `--confirm` 只读生成报告；确认后重新校验、备份并原子记录真实来源。接入要求旧 Schema 快照、实际结构及旧历史一致，存量数据满足目标约束。跳过含数据操作的正式步骤，还必须给出逐步 `baseline_proofs`，每项包含 `step` 和对应只读 `assertions`；无法证明语义则拒绝。新空库不能借接入绕过完整链。

接入不是补写“已执行全部历史”：旧历史原样保留，正式基线来源独立记录，后续真实步骤从基线继续。开发快照明确拒绝接入；来源写入数据库内部，不能通过只复制 SQLite 文件、丢掉旁路标记来伪装旧库。

## dev、快照、停服与恢复

持久化 dev 与 run 使用相同正式规则，不因模式切换重置数据库。普通 UI 热更新保持原流程；正式链或物理 Schema 改变须停服再启动，未提供迁移时不修改现有库。一次性开发快照必须显式指定隔离空路径，详见 [配置契约](../contracts/project-configuration.md#一次性开发快照)；run 不接受快照产物或数据库。页面结构/标题改变仍可能重置尚未提交的表单草稿，持久化数据不因此重建；本轮未改动前端表单的代际重置规则。

锁文件 `<数据库>.kenvoy.lock` 保持在原位，锁随进程/连接线程退出由 OS 释放，不通过删除锁文件解锁。新协议不能阻止旧 CLI 或外部 SQLite 客户端访问；部署流程仍须确认旧服务退出，并使用同一规范数据库路径，不以 SQLite 写锁代替生命周期协调。

恢复备份是独立运维操作：先停止写入，保存原数据库、WAL/SHM 和日志，核验备份完整性，再针对具体对象及覆盖后果确认。没有自动 restore、自动降级、跨库两阶段事务、PostgreSQL 或多实例滚动升级。本轮验证范围与已知限制见 [实施记录 0026](../changes/0026-application-migration-lifecycle.md)。
