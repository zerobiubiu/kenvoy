# SQLite、Schema 与迁移设计

状态：参考（现状文档）；核对日期：2026-11-02。本篇维护连接所有权、Schema 与数据一致性；命令操作和确认步骤见[迁移执行协议](../operations/migration-design.md)。首版证据见[最终验收](../verification/0012-final-acceptance.md)，附件及 dev→run 交叉读写见[0014 专项验收](../changes/0014-file-uploads-and-images.md)。

## 职责与连接

全部实现位于 `crates/kenvoy/src/infrastructure/persistence/sqlite/`，不进入 KEngine。输入为已验证 Application IR 和正式交付链；加载产物仍复核资源/字段名、默认值和约束。默认数据库为项目根的 `data/app.sqlite`，独立于构建目录。迁移状态、历史和基线预览只读现有数据库，路径不存在不会创建文件与目录。

`Database` 是可克隆异步句柄，专属线程独占 rusqlite Connection，Tokio 队列容量 64。正式初始化先验证交付链，取得 OS 生命周期锁，再设置 foreign_keys=ON、WAL、busy_timeout=5000ms、synchronous=NORMAL 并逐步迁移。锁定 rusqlite 0.41.1 bundled/backup/hooks，hooks 用于受限数据规则 SQL 的授权；WAL 不代表网络盘或多写者部署已受支持。SQLite 写锁只保护事务，不替代覆盖服务运行期的生命周期锁。

队列满返回 Busy，表示请求尚未接受；已接受写入不施加声称“已取消”的响应超时。丢弃响应不取消已接受操作，线程异常导致响应丢失返回 WorkerLost，结果需核对。显式 shutdown 拒绝新任务并等待已接受任务，超过 6 秒请求 SQLite 中断及取消未执行队列，再有界等待 6 秒；返回 ShutdownTimeout 时保留线程句柄，不能宣称已释放。Drop 只是兜底，正常宿主必须显式关闭并检查结果。

## Schema 与字段

`migration::schema_model` 将资源转为排序的 SchemaModel/TableModel/ColumnModel/IndexModel；标签、源码位置、应用名称及字段显示顺序不影响 SHA-256 Schema hash。textarea 与 text 数据库类型相同，枚举按值排序，显式 null 默认值与缺省默认值分别保留。索引名确定且全局检查冲突。

| 字段 | 存储 | 约束 |
|---|---|---|
| text/textarea/select | TEXT | 类型、required、Unicode 标量长度和 NUL 拒绝；静态 select 另有全集枚举 CHECK |
| number integer | INTEGER | 安全整数 ±(2^53−1)、范围，禁止舍入或截断 |
| number | REAL | 有限数和范围 |
| boolean | INTEGER | 0/1；CRUD JSON 仍为布尔值 |
| date | TEXT | 日历日期，服务层严格验证 |
| datetime | TEXT | 显式时区输入规范化为 UTC 毫秒 |
| file | JSON TEXT 元数据数组；内容位于同库 BLOB | 数组、required、数量及元数据约束；写事务核验真实归属，不接受标量筛选/排序 |
| 系统字段 | INTEGER id；TEXT created_at/updated_at | 自增主键、UTC 毫秒时间；不接受业务输入赋值 |

业务名仅允许小写 ASCII 字母开头及后续字母、数字、下划线，拒绝 sqlite_、kenvoy_、系统字段和 auth/actions 等保留名。SQL 标识符来自受检注册表，输入值使用绑定参数；DDL 常量使用受控模板和严格转义。required 文本 SQL CHECK 与 Rust 使用一致 Unicode White_Space 集合，并拒绝 NUL，避免 SQLite 文本长度遇 NUL 截断。空资源、没有可表示整数的 integer 区间、过大 maxLength、冲突约束和非法默认值在创建数据库前拒绝。

CRUD 复用 validation 的 Create/Patch 规则，提供 create/get/list/update/delete；写入及结果读取在事务中完成。列表默认每页 20，上限 100，最多 16 个筛选条件，搜索最多 256 个 Unicode 标量。排序只允许可排序声明字段及系统字段，以 id 补充稳定顺序；file 不接受标量筛选/排序，也不参与全文搜索。LIKE 转义百分号、下划线和反斜杠。计数和分页读取使用同一事务快照。未知资源、内部表、未知字段和非法查询拒绝。

数据库 Runtime 属于内部能力，HTTP 先检查用户与 Resource 策略，再调用数据库；持有句柄不代表公开 API 已经授权。

远程 select 的值仍是 TEXT，不生成静态 IN CHECK。选项查询仅取已声明的值/标签列，条件使用参数绑定，分页和 selected 值解析在同一读事务内。创建与 PATCH 在写事务内验证来源值和联动条件；PATCH 先合并已有记录。when/筛选等运行规则不参与物理 Schema hash，规则变更保留存量数据、后续写入重验；静态枚举全集变化或切换远程方式按实际约束 Diff 处理。详见[组件与选项契约](../contracts/component-bindings.md)。

## 附件一致性

附件通过同一专属连接线程管理，二进制与元数据保存在固定内部表；暂存归属会话摘要，绑定归属资源/字段/记录。CRUD 的校验、绑定、替换清理和记录删除共用写事务；已确认的字段/资源删除与附件清理共用迁移事务，失败保留旧状态，备份包含附件。启动及上传回收过期暂存，不创建公开文件目录。限额、访问接口及明确限制见[文件与图片契约](../contracts/file-uploads.md)。

## 正式交付链、Diff 与历史

项目 `migrations/` 保存清单、追加步骤和候选；构建产物保存完整 `migrations.json`。步骤格式 1 使用版本化 `sqlite-schema-v1` 执行器，冻结前序、说明、from/to hash、目标 Schema、规则、风险和 checksum。它不调用当前应用函数，不依赖部署源码。Schema hash 契约 `sha256-schema-v1` 保持既有字节算法，规范模型仍按排序 JSON 计算；数据迁移有独立步骤，即使 hash 相同也执行并记录。

`generate` 比较上一正式 Schema 与受检 IR，连接既有 Diff 能力生成候选；`seal` 检查风险处理、显式映射、数据规则和内存 SQLite 执行结果后追加固化。危险候选需要内容 checksum 确认；运行时另使用绑定数据库当前状态和待办链的完整执行计划 checksum。两种确认不可互换，均不能代替约束核验。精确规则结构见[迁移操作](../operations/migration-design.md)。

正式 `run`、持久化 `dev`、`migrate apply` 和账号管理都使用正式链打开数据库；不再按最新 IR 临时推导并执行 DDL。空库也逐步执行完整历史；目标与当前源码不一致时先报告缺少迁移，不能删库、重建或伪造已执行记录。启动对照实际结构和已提交历史，必须证明数据库是目标链有效前缀；领先、缺失、改写、重复或分叉均停止对外服务。

`_kenvoy_schema` 保存真实结构版本、规范 Schema JSON/hash 与实际结构快照；`_kenvoy_migrations` 保存每次真正执行的 checksum、from/to hash、plan、备份与时间，包括同结构的数据步骤。新正式交付状态由 `_kenvoy_catalog_owner`、`_kenvoy_catalog_applied`、`_kenvoy_catalog_baseline` 独立记录；每步的 DDL、数据、旧结构审计和正式成功位置在同一事务提交。旧库基线只记录真实来源，不向 applied 填入未执行步骤，原审计历史保持不变。

实际结构来自排序的 sqlite_master 对象。迁移、正式交付、快照来源、认证和附件 owner 分别精确核验自身 DDL、版本与记录；只有完成核验的已知对象才排除，未知同前缀表/索引/视图/触发器仍阻断。数据规则 SQL 使用 SQLite authorizer 限制业务对象访问与事务权限，不能改写宿主元数据或挂载外部数据库。冻结数据校验直接复用建表 CHECK 谓词，逐行核验 NOT NULL、系统 ID/时间和 UTF-8 字节，不能仅靠结构或 integrity_check 声称数据有效；附件还验证元数据、实际 BLOB 和归属守恒，文件字段改名同步更新绑定。

## 旧库基线与开发快照

缺少正式交付状态的现存库不是空库。显式 baseline 预览同时核验旧 Schema/实际结构/历史、选定正式步骤和数据约束；跳过含数据操作的历史要求逐步语义证明断言。不能证明则停止。确认 checksum 后先备份，再原子记录真实接入来源，后续真实迁移从基线继续。新空库不能 baseline 绕过链。

一次性开发快照必须显式指定与正式数据隔离的空路径，直接建立最新结构，并先提交数据库内 `_kenvoy_dev_snapshot` 来源标记，同时保留初始化保护 sidecar；正式 run、迁移和 baseline 均拒绝快照，单文件复制或改名不改变来源。该内部表同样进行精确 owner 校验，不忽略未知同前缀对象。切换模式从不隐式重置数据库或转换历史，也不会默认占用共用 data/app.sqlite。

## 备份与回滚

危险计划只允许持久化数据库，传入路径必须与连接一致。在生命周期锁和事务保护下通过独立只读连接调用 SQLite backup API，将一致性备份写入数据库父目录 backups/，限制 30 秒，完成后 quick_check 必须为 ok；WAL 活动时不直接复制主文件。备份或预检失败不执行该步；先前成功步骤保持提交，失败备份文件不能未经核验当作恢复点。

现有记录逐字段验证目标约束；唯一字段检查重复。重建时创建受控临时表、复制保留列及系统字段、核对行数、替换旧表并恢复索引，所有步骤受事务保护。保留 AUTOINCREMENT 最高已用序号，避免删除最高记录后重建导致 ID 重用。显式删除数据留在变更前备份，管理员可停服后恢复，目前没有自动 restore 命令。

当前测试覆盖唯一冲突、约束/断言失败、非法 UTF-8、分步提交恢复以及附件字段改名的 BLOB/绑定回滚。早期借伪造认证表注入业务外键的测试已退役，新 owner 校验会在 DDL 前拒绝该状态，不放宽内部表所有权来恢复旧测试。磁盘耗尽、断电和备份目录 IO 故障尚未注入测试，不将事务用例等同于这些故障全部通过。

## 模型替换与边界

正式运行和开发快照中的 replace_application 只接受相同物理 Schema；改变正式链或 Schema 要求停服重启，不在热更新期间执行 DDL。HttpRuntime::publish_prepared 的同代写锁继续协调模型、JavaScript 与静态资产，失败保留原注册表。只用于低层隔离测试的旧动态 Diff 执行器仍验证原有数据保护，不作为生产启动入口。

应用版本、构建哈希与就绪/失败另追加到数据库相邻启动日志，迁移位置从 SQLite 提交状态读取。迁移全部成功但激活失败明确说明数据库已迁移，不能宣称回滚；恢复备份是独立操作，启动不自动覆盖业务库。关系字段、网络盘、多实例滚动升级、磁盘耗尽/断电与大数据量耗时/空间预算不在当前实测承诺内。

证据见 [阶段 04 验证](../verification/0006-sqlite-runtime.md)、[实施记录](../changes/0005-sqlite-runtime.md)；相关决策为 [ADR 0005](../decisions/0005-sqlite-database.md)，生命周期见 [Runtime](runtime-design.md)。
