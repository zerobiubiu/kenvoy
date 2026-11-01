# SQLite、Schema 与迁移设计

状态：现行；核对日期：2026-11-01。本篇维护连接所有权、Schema 与数据一致性；命令操作和确认步骤见[迁移执行协议](migration-design.md)。真实 CRUD、HTTP/浏览器及 dev/run 共用 SQLite 已通过[最终验收](verification/0012-final-acceptance.md)。

## 职责与连接

全部实现位于 `crates/kenvoy/src/database/`，不进入 KEngine。输入为 Application IR；重载构建产物时仍复核资源名、字段名、默认值和约束。默认路径为项目根的 `data/app.sqlite`，独立于构建目录。迁移预览和历史只打开已有数据库；路径不存在时返回初始计划或空历史，不创建文件与目录。

`Database` 是可克隆异步句柄，专属线程独占 rusqlite Connection，Tokio 队列容量 64。初始化设置 foreign_keys=ON、WAL、busy_timeout=5000ms、synchronous=NORMAL。锁定 rusqlite 0.41.1 bundled/backup，当前 Windows MSVC 实测 SQLite 3.54.1。同步 SQL 不在异步请求线程执行；WAL 用于本机持久化文件系统，不据此宣称支持网络盘或多写者并行执行。

队列满返回 Busy，表示请求尚未接受；已接受写入不施加声称“已取消”的响应超时。丢弃响应不取消已接受操作，线程异常导致响应丢失返回 WorkerLost，结果需核对。显式 shutdown 拒绝新任务并等待已接受任务，超过 6 秒请求 SQLite 中断及取消未执行队列，再有界等待 6 秒；返回 ShutdownTimeout 时保留线程句柄，不能宣称已释放。Drop 只是兜底，正常宿主必须显式关闭并检查结果。

## Schema 与字段

`migration::schema_model` 将资源转为排序的 SchemaModel/TableModel/ColumnModel/IndexModel；标签、源码位置、应用名称及字段显示顺序不影响 SHA-256 Schema hash。textarea 与 text 数据库类型相同，枚举按值排序，显式 null 默认值与缺省默认值分别保留。索引名确定且全局检查冲突。

| 字段 | 存储 | 约束 |
|---|---|---|
| text/textarea/select | TEXT | 类型、required、Unicode 标量长度、枚举和 NUL 拒绝 |
| number integer | INTEGER | 安全整数 ±(2^53−1)、范围，禁止舍入或截断 |
| number | REAL | 有限数和范围 |
| boolean | INTEGER | 0/1；CRUD JSON 仍为布尔值 |
| date | TEXT | 日历日期，服务层严格验证 |
| datetime | TEXT | 显式时区输入规范化为 UTC 毫秒 |
| 系统字段 | INTEGER id；TEXT created_at/updated_at | 自增主键、UTC 毫秒时间；不接受业务输入赋值 |

业务名仅允许小写 ASCII 字母开头及后续字母、数字、下划线，拒绝 sqlite_、kenvoy_、系统字段和 auth/actions 等保留名。SQL 标识符来自受检注册表，输入值使用绑定参数；DDL 常量使用受控模板和严格转义。required 文本 SQL CHECK 与 Rust 使用一致 Unicode White_Space 集合，并拒绝 NUL，避免 SQLite 文本长度遇 NUL 截断。空资源、没有可表示整数的 integer 区间、过大 maxLength、冲突约束和非法默认值在创建数据库前拒绝。

CRUD 复用 validation 的 Create/Patch 规则，提供 create/get/list/update/delete；写入及结果读取在事务中完成。列表默认每页 20，上限 100，最多 16 个筛选条件，搜索最多 256 个 Unicode 标量。排序只允许声明字段及系统字段，以 id 补充稳定顺序；LIKE 转义百分号、下划线和反斜杠。计数和分页读取使用同一事务快照。未知资源、内部表、未知字段和非法查询拒绝。

数据库 Runtime 属于内部能力，阶段 04 尚不执行 HTTP 用户授权。后续 HTTP 必须先检查用户与 Resource 策略，再调用数据库；持有句柄不代表公开 API 已经授权。

## Diff、确认与历史

`preview(path, app)` 返回 MigrationPlan，含数据库绝对规范路径、当前版本、前后 Schema hash、实际结构 hash、变更、破坏性标志和 checksum。`apply_connection` 在 BEGIN IMMEDIATE 中重新读取状态并生成计划；确认 checksum 不匹配即拒绝，不能用全局 yes 替代具体计划。

| 变化 | 当前规则 |
|---|---|
| 新表、新可空字段、有合法非 null 默认值的必填字段 | 安全计划，经预检后可应用 |
| 新普通/唯一索引 | 普通索引安全；唯一索引先检查已有重复值 |
| 删除表/字段/索引、类型、约束、默认值变化 | 显式确认当前 checksum，先备份及预检 |
| 无默认值必填增列 | 不自动应用；确认后已有数据仍须符合约束 |
| 单一删除与同定义新增字段 | 报告 RenameCandidate；即使确认仍拒绝猜测，映射执行尚未实现 |

空 Diff 不执行 DDL、不新增历史。删除 TSX 字段不会默认自动 DROP；未确认时保留旧库和旧有效模型。类型变更不做隐式字符串/数字转换；不兼容既有值不能靠确认跳过检查。

`_kenvoy_schema` 保存版本、规范 Schema JSON/hash、实际结构 JSON/hash；`_kenvoy_migrations` 保存递增版本、checksum、from/to hash、完整 plan_json、backup_path、执行时间。历史和 DDL 同事务提交，校验版本连续、前后 hash 链、计划 checksum 与最终 Schema。

实际结构快照来自排序的 sqlite_master 对象定义，包含业务表、索引、视图和触发器。手工修改、未知内部表、缺失元数据或记录篡改均阻断；迁移两张表另外核验精确受控 DDL。仅预留精确表所有权 `_kenvoy_users`、`_kenvoy_sessions`、`_kenvoy_auth_schema` 给认证模块，由其独立核验版本与结构，绝不忽略所有同前缀对象。已有未纳管业务表拒绝自动接管。

## 备份与回滚

破坏性计划只允许持久化数据库，传入路径必须与连接一致。获取写锁后，通过独立只读连接调用 SQLite backup API，将一致性备份写入数据库父目录 backups/before-版本-时间-进程.sqlite，限制 30 秒，完成后 quick_check 必须为 ok；WAL 活动时不直接复制主文件。备份后预检失败仍保留备份；备份失败不执行迁移，失败生成的文件不能未经核验当作恢复点。

现有记录逐字段验证目标约束；唯一字段检查重复。重建时创建受控临时表、复制保留列及系统字段、核对行数、替换旧表并恢复索引，所有步骤受事务保护。保留 AUTOINCREMENT 最高已用序号，避免删除最高记录后重建导致 ID 重用。显式删除数据留在变更前备份，管理员可停服后恢复，目前没有自动 restore 命令。

真实测试覆盖唯一冲突、约束失败和执行中外键阻止 DROP：先前创建的新表、历史及业务数据一起回滚。磁盘耗尽、断电和备份目录 IO 故障尚未注入测试，不将事务测试等同于这些故障全部通过。

## 模型替换与边界

replace_application 在工作队列串行迁移，成功后替换 Application IR 并增加数据库 generation，失败保留原注册表。HttpRuntime::publish_prepared 持应用写锁，在迁移成功后同步替换模型、JavaScript 包与静态资产；已接受的发布由独立跟踪任务完成，调用者取消不使状态中断。Development 在屏障前准备客户端候选，之后更新镜像和活动清单；提交后的收尾失败不能声称数据库回滚，具体策略见 [运行时设计](runtime-design.md)。

关系/文件字段、改名映射、跨代际崩溃恢复、大表耗时和空间预算仍待后续设计。预检按行读取，但唯一值集合占用与数据量相关的内存，需大表实测后优化。本阶段只迁移临时测试文件，没有迁移用户生产数据。

证据见 [阶段 04 验证](verification/0006-sqlite-runtime.md)、[实施记录](changes/0005-sqlite-runtime.md)；相关决策为 [ADR 0005](adr/0005-sqlite-database.md)，生命周期见 [Runtime](runtime-design.md)。
