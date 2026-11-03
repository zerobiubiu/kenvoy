# 0026 应用兼容、正式迁移与运行生命周期

状态：已执行；治理：不可变变更记录。

- 实施日期：2026-11-03。
- 影响范围：Kenvoy CLI、应用配置、SQLite 迁移、产物、开发/生产生命周期、示例和验收。
- 前置：[0005 SQLite](0005-sqlite-runtime.md)、[0010 生命周期](0010-cli-lifecycle.md)、[0025 优雅退出](0025-graceful-shutdown.md)。
- 替代关系：替代生产启动/持久化 dev 根据最新 IR 临时生成并执行 Diff 的行为；保留原结构历史，不重写历史文档。
- 发布状态：仅 dev 本地实施；未机械升版、未合并 main、未推送或发布。Kenvoy 仍为 0.2.1，KEngine 保持 0.2.0；后续版本按[发布流程](../operations/releasing.md)确定。

## 背景与边界

原有 Diff、checksum、备份、事务和实际结构快照只能说明数据库曾应用哪些动态计划，不能提供随应用交付、无源码也可执行的正式历史。应用版本、Schema hash、CLI 能力和实际数据库迁移位置也需要独立管理。

本次按实施计划完成配置/产物兼容、正式链生成与固化、顺序执行、显式旧库接入、生命周期互斥、一次性开发快照和启动状态记录。迁移仍归属 Kenvoy，不增加 Workspace 成员，不进入 KEngine；不实现 PostgreSQL、多实例滚动升级、通用 ORM、自动恢复或自动降级。

## 方案与公共契约

- 应用 `project.version`、最低 CLI、项目格式、构建哈希、Schema hash 和迁移 ID/checksum 各有独立职责。完整 SemVer 使用 precedence 比较，忽略 build metadata；当前项目格式支持 `>=0.2.0, <0.3.0`，稳定 CLI 拒绝预发布的最低 CLI/格式。旧配置不自动补字段或提高数字，`migrate history` 保留独立诊断入口。
- 产物协议升级至 2，保留框架精确版本/指纹门禁；增加项目身份与兼容字段、哈希算法版本和完整 `migrations.json`。无源码运行仍验证全部文件、链和数据库，不能靠配置改版本冒充其他产物。
- 正式迁移位于 `migrations/manifest.json`、追加步骤及 `draft.json`；格式 1，执行器 `sqlite-schema-v1`。步骤冻结 Schema、映射/回填/转换/SQL/断言、风险等级和执行器要求，不调用当前业务函数。危险候选固化与数据库执行分别确认各自 checksum。
- `run`、持久化 `dev` 和 `migrate apply` 只消费固化链。空库也从第一步执行；每步结构、数据、旧审计和正式成功位置同事务提交。失败诊断重新核对真实位置，保留前序提交，不宣称所有步骤已回滚。
- 旧库基线先只读报告、语义断言与精确确认，再备份和原子登记真实来源；不向已执行表填入跳过步骤。正式前缀、元数据 owner、实际结构、冻结字段数据、UTF-8 和附件绑定均核验。
- OS 文件锁覆盖迁移和整个数据库线程/服务期；正式数据、源码、固定迁移目录和输出目录必须隔离。新锁不承诺协调旧 CLI 或外部 SQLite，部署仍须先停旧服务。
- `dev --snapshot` 仅接受隔离的空路径，先写入数据库内 `_kenvoy_dev_snapshot` 来源标记，并保留初始化保护 sidecar；单独复制数据库也不能经 baseline 转正。持久化 dev 的链/物理 Schema 变化要求停服重启，普通 UI 更新沿用原流程。
- 应用启动与迁移位置分开记录：`<数据库>.kenvoy-startups.jsonl` 包含应用版本、build_id、CLI、迁移位置和 migrated/ready/失败/停止结果。迁移成功但监听失败明确报告“数据库已迁移，应用未就绪”，不自动覆盖备份。

完整现行契约见[配置](../contracts/project-configuration.md)、[数据库设计](../architecture/database-design.md)、[迁移操作](../operations/migration-design.md)和[Runtime](../architecture/runtime-design.md)，不在本记录重复完整文件格式。

## 实施文件清单

| 范围 | 文件与职责 |
|---|---|
| 兼容与路径 | `infrastructure/project/{config,compatibility,compatibility_tests,snapshot,mod,create}.rs`；四个示例的 `kenvoy.toml` |
| 构建交付 | `infrastructure/artifacts/{mod,fingerprint,load,staging,tests,catalog_tests}.rs`；v2 身份、输入范围、冻结链和无源码校验 |
| 正式迁移 | `infrastructure/persistence/sqlite/migration/catalog/` 的 model/storage/rules/validate/sql/executor/records/delivery/inspection/apply/baseline 及分模块测试；`migration/{mod,state,planning}.rs` |
| 冻结数据规则 | `schema/{mod,ddl,frozen,frozen_data}.rs`；原 CHECK 单点复用、实际结构与流式数据/UTF-8 核验 |
| 数据生命周期 | `sqlite/{lifecycle,snapshot,identity,mod}.rs`、`worker/{mod,opening,resources}.rs`、`uploads/{mod,migration}.rs`；锁、快照来源、只读 owner、附件改名与引用守恒 |
| CLI 装配 | `bootstrap/{commands,migrations,build,server,startup,lifecycle,error,mod}.rs`、`interfaces/cli/args.rs` |
| 验收 | `crates/kenvoy-tests/tests/migration_contract.rs` 及 `migration_support/`；CLI migrations/dsl_security 测试；worker delivery/snapshot_origin、project regression；浏览器 fixture 及 fields/options/watcher 脚本 |
| 示例和依赖 | 四个 `examples/*/migrations/` 初始正式链；根与 CLI Cargo manifest、Cargo.lock、运行元数据 ignore |
| 文档 | 配置、数据库、迁移、Runtime、CLI、测试策略、依赖登记、README、版本日志和各级索引 |

`semver 1.0.28` 来自已缓存 crates.io 包，锁文件只增加该依赖；rusqlite 0.41.1 保留 bundled/backup 并启用 hooks 进行 SQLite 原生授权。未安装新系统工具，未修改系统安全、浏览器启动或 Cargo 持久配置。

## 验收覆盖与修复闭环

13 个跨层场景使用真实 CLI、HTTP 和临时 SQLite，覆盖全部指定验收：完整空库链、有数据升级、幂等、同 Schema 数据步骤、历史改写/缺失/分叉、独立格式拒绝、未固化 Schema、分步失败恢复、迁移后启动失败、两个进程争用、无源码部署及 baseline/快照隔离。

审查及实际失败暴露的边界另保留永久回归：

1. 数据目录可配置到 `migrations/`，指纹/监听会排除正式链。失败用例复现后，统一拒绝源码、数据或输出与固定迁移目录重叠。
2. 仅有 sidecar 的快照可被单文件复制后当成旧库。失败用例复现后，将来源写入 SQLite 内，正式入口和 baseline 均核验，保留业务记录而拒绝转正。
3. `CAST(X'80' AS TEXT)` 可提交 SQLite 接受、应用无法解码的文本。失败用例复现后，在冻结行校验中检查 UTF-8，当前步回滚且旧记录仍可读取。
4. 本机独立只读连接的 integrity_check 曾返回 ok，而原连接报告既有 CHECK 违规；不能用该结果代替目标语义证明。共享原 DDL 谓词直接核验存量行后，非法旧库基线用例通过。
5. 文件字段显式改名同步更新 BLOB 归属；后续校验失败时，业务列、附件引用和宿主绑定一起回滚，修复前提后从原位置恢复。

早期伪造认证表以制造业务 FK 的用例已退役，不能放宽 owner 校验来迁就其不可达前提；事务失败和附件数据保护由真实正式迁移用例覆盖。CLI 删除字段夹具改为显式处理风险后再分别确认固化和执行，删除对具体错误措辞的偶然断言。

## 执行验证记录

### 环境

Windows x64；Rust/Cargo 1.99.1；Node 24.22.0；npm 12.2.0；实际 `DatabaseInfo.sqlite_version` 为 3.54.1。浏览器使用当前配置的 ego lite，不另起自定义 Chromium。所有数据库与进程均为自有临时资源；未用示例或用户业务库试验迁移。

### 命令、实际输出与结论

| 命令/操作 | 实际输出 | 退出/结论 |
|---|---|---|
| `cargo metadata --offline --format-version 1` | `Locking 1 package`、`Adding semver v1.0.28` | 0；锁定缓存依赖 |
| `cargo run --package xtask --offline --locked -- test` | Kenvoy 单元 `167 passed; 1 ignored`，真实 `migration_contract` 为 `13 passed`；其余 Workspace 常规用例通过 | 0；含真实 CLI 构建、SQLite/HTTP、旧库、无源码和互斥验收；不重复累计 watchdog 子进程计数 |
| 兼容性夹具去掉偶然版本常量后的定向 `migration_contract` 用例 | `1 passed; 12 filtered out` | 0；仍独立证明最低 CLI/项目格式拒绝 |
| 一次性 `throwaway_column_rule_equivalence` | `DDL差分：50000条；0差异` | 0；与已提交 DDL 规则逐字结果一致，探针模块及文件已移除 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | `Finished dev profile` | 0；无 allow 或放宽门禁 |
| `cargo run --package xtask --offline --locked -- architecture` | 四成员/依赖方向通过；最大源码 449 行，硬上限 500；388 个自有源码文件 | 0；无新增成员或边界豁免 |
| `cargo run --package xtask --offline --locked -- check` | 架构通过；97 份 Markdown、981 个本地链接通过 | 0；现状契约、实施记录与分类索引完整 |
| `node --check` 逐个检查 fixture.mjs、fields/options/watcher-e2e.mjs | 无语法错误输出 | 0；仅证明脚本语法，不作为浏览器流程执行通过 |
| 四个示例 `migrate generate` + `migrate seal` | customer/extensions/options/uploads 均固化位置 `000001` | 0；仅生成交付文件，未访问示例数据库 |

首次编译发现 SQLite 参数不能直接绑定 usize/u64，已做有边界检查的 i64 转换；随后测试修复与上述闭环一并完成。首次严格 Clippy 的多余生命周期和复杂返回类型已修正，没有抑制告警。

### 实际运行与浏览器冒烟

1. 用当前真实 CLI 创建独占临时应用，执行初始链，插入客户 1（年龄 28）。
2. 源码增加 `migration_marker`，generate/seal 后故意占用端口再 run：退出 1，真实位置为 `000002`，旧记录仍为年龄 28、新字段 null；启动事件只有 migrated/startup_failed，没有伪造 ready。
3. 释放端口后启动 dev，通过配置的 ego 浏览器访问真实表单。修改页面标题后已显示“迁移验证登记”，再填写并提交记录 2，页面显示“提交成功、记录编号 2”。SQLite 实读客户 1 未变，客户 2 年龄 35、迁移标记“完整迁移后提交”，正式历史仍为两步；UI 更新没有隐式迁移。
4. 当前表单代际规则会在页面标题/结构更新后重置未提交草稿，首次探针对此的额外断言失败；本轮没有更改该前端规则，也不声称所有 UI 热更新都会保留草稿。
5. 停止自有服务后，启动日志追加 stopped，另一 CLI 成功重新取得数据库使用权；旁路监督器报告的 PTY 退出码为 512，而非可确认的应用退出 0，已作为工具状态差异反馈，不扩大为完整 Windows 信号验收。
6. 浏览器任务已关闭，临时应用与一次性脚本已移除。视觉证据保留在本机忽略目录 `artifacts/migration-lifecycle-smoke/smoke.png`。

### 已知限制与后续验证建议

- 普通工程测试忽略 7 套 Playwright 浏览器流程及 1 项需要实际用户 npm 安装的自定义构建用例；本轮通过配置的 ego 浏览器完成上述真实表单/热更新/写库冒烟，没有把 ignored 算作通过。
- 后续一次定向测试出现 Windows 增量缓存 finalize 的 os error 5 提示，但测试退出 0；未修改 ACL、索引服务、杀软或持久增量设置，也不据此宣称缓存问题已修复。
- 语言服务器启动失败，未能提供 LSP 引用查询；导出变化按精确调用范围迁移，并由全目标编译、Clippy 和行为回归核验。
- 未运行当前修订的 Rust 1.98、Linux/macOS、真实断电/磁盘满、网络盘、大库时空预算或多实例滚动升级。应在独立目标环境按相同验收场景补证，不使用生产库故障注入。
- 恢复备份仍是需要停服、明确对象和覆盖后果的独立操作；没有自动恢复、降级或快照转正通道。
