# 阶段 04：SQLite Runtime 与事务迁移

状态：已执行；环境：Windows MSVC、Rust 1.99.1；日期：2026-11-01。前置：[阶段 03](0005-dsl-cli.md)。

## 已验收能力

真实文件 SQLite 初始化、WAL/FK/busy_timeout、数据库线程和有界队列、受控资源 CRUD、分页/搜索/排序/筛选、关闭与重开持久化已实现。Resource 来自 Application IR，内部表和非法资源名不能通过通用 CRUD 访问；尚未接通正式 HTTP 认证授权。

迁移实现规范 Schema/hash、只读预览和历史、建表、安全增列/索引、删除/类型/约束/候选改名检测、checksum 确认、SQLite backup、实际结构与元数据漂移检查、重建和事务回滚。记录与 DDL 同事务提交，重复应用不新增版本；改名映射执行明确拒绝。

CLI migrate 默认预览、apply、history 使用真实 Runtime；预览不创建数据库，apply --confirm CHECKSUM 绑定具体计划。测试迁移均位于临时目录，未操作用户业务数据库。

## 实际验证

| 命令或定向检查 | 观察结果 |
|---|---|
| cargo check --workspace --offline --locked | 退出 0 |
| cargo test --workspace --offline --locked | 退出 0，统一验证时 145 项常规测试通过，1 项既有跨工具浏览器探针忽略 |
| cargo clippy --workspace --all-targets --offline --locked | 退出 0；当时 22 个尚未由正式 HTTP 消费的 CRUD 等 dead_code/unused 警告，未使用 allow 或虚构 CLI 消除 |
| cargo test -p kenvoy --bin kenvoy database::migration::tests --offline --locked | 最后复验退出 0，11 项通过；含整数区间/maxLength/空资源复核 |
| cargo test -p kenvoy --bin kenvoy database::worker::tests --offline --locked | 夹具修复后退出 0，当次 10 项通过 |
| cargo test -p kenvoy --test migrations_cli --offline --locked | 退出 0，2 项真实 Binary 集成通过 |
| 并行独立数据库启动、重开、关闭定向压力测试 | 退出 0；新增 1 项，8 并发 × 6 个独立数据库、96 次线程生命周期 |
| node tools/verify-docs.mjs | 本次文档落盘后退出 0，42 个 Markdown、144 条本地链接通过 |

145 项包含原阶段 03 的 121 项、迁移 11 项、工作线程 10 项、CLI 迁移 2 项及 DSL NUL 约束回归 1 项。随后新增压力测试单独通过，累计 146 项不同常规测试已有通过记录；不将“145 项全量 + 1 项定向”写成执行过一次 146 项全量命令。共享字段向量新增 NUL 场景 3 个，目前共 68 个。

迁移最后定向验证使用 CARGO_INCREMENTAL=0，文件格式命令 rustfmt --edition 2024 crates/kenvoy/src/database/migration.rs 退出 0。最终整体格式、文档链接与架构检查由阶段总门禁执行，本表不替代后续记录。

## 测试细节和修复

迁移覆盖：无副作用预览、建表及重放、持久化、默认/可空字段和索引、唯一预检、过期确认、备份保留删列、AUTOINCREMENT 高水位、类型/约束/改名拒绝、手工表/索引/未知内部表/历史/元数据触发器漂移，以及真实外键 DDL 失败后先前 DDL 与历史一起回滚。

工作线程覆盖七类字段、默认值与 Patch 缺省区别、查询注入/LIKE/分页、唯一冲突、失败迁移保留旧模型、并发写入和队列拒绝、已接受写入丢弃响应后仍提交、安全整数 ID 溢出回滚、幂等关闭及克隆关闭、长 SQL 中断回收。新增独立数据库压力测试验证同时初始化与重开。

早期编译发现 sha2 0.11 摘要输出不支持 LowerHex、rusqlite 本版 u64 不支持 ToSql/FromSql、column_names 返回迭代器；改为逐字节十六进制、受检 i64 转换、直接迭代后通过，未放松检查。

工作线程早期出现一次 SQLite Busy，后续未能稳定复现，不能认定根因已证实。夹具只使用 pid+时间戳并以 create_dir_all 建目录，存在重名时静默共享的确定性缺陷；改为递增序号与排他 create_dir，修复后 10/10 及新增 48 数据库并行压力测试通过。保留失败事实，后续 HTTP 并发继续观察。

新增 sha2 0.11.1 及锁定依赖，共新增 7 个 Cargo 包；rusqlite 0.41.1 bundled/backup 与 Tokio 从已有探针依赖进入正式宿主依赖，实际 SQLite 为 3.54.1。来源为 crates.io，位置为已有 Cargo 缓存，Cargo.lock 固定版本，没有新增 Workspace 成员。

## 限制与后续验证

本阶段不是浏览器闭环。正式 HTTP/Session/授权、React/Admin、JavaScript 函数及 dev/run 尚未由本阶段实现。后续验证请求大小、CSRF、授权、错误脱敏、HTTP 并发及优雅停机。

改名映射、自动恢复、磁盘耗尽/断电注入、大表预算未完成。备份后事务失败保留完整备份；备份过程中失败的文件须另行核验，不能当作恢复点。没有对用户业务数据执行破坏性迁移。
