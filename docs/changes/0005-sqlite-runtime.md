# SQLite 资源 Runtime、迁移与命令

状态：已执行；日期：2026-11-01；前置：[DSL 与基础命令](0004-dsl-cli.md)。

## 背景与方案

将阶段 03 的 Resource 验证契约接入真实文件 SQLite，避免建立后台/API 专用模型。数据库能力全部属于 Kenvoy，不改变 KEngine 的通用边界。采用专属连接线程、有界队列、受检资源注册表，迁移结合实际结构、规范 Schema 和可校验历史判断状态。

## 实施与文件

新增 crates/kenvoy/src/database/ 下 mod.rs、worker.rs、query.rs、migration.rs、worker_tests.rs，实现生命周期、参数化 CRUD、查询限制、Schema Diff、确认、备份和事务迁移。新增 tests/migrations_cli.rs，通过真实 Binary 和临时目录验证预览、应用、历史与失败。

修改根 Cargo.toml、Cargo.lock、kenvoy/Cargo.toml：sha2 0.11.1 及传递依赖新增 7 个包；已有 rusqlite 0.41.1 bundled/backup 与 Tokio 接入正式依赖，来源为 crates.io、位置为已有 Cargo 缓存。main.rs 接通 migrate 默认预览、apply 和 history，不生成按资源分拆的 CRUD 源码。

修改 validation.rs、DSL 约束和 tests/fixtures/validation.json，拒绝 NUL 文本/枚举，新增 3 个共享向量，目前 68 个。Schema 产物重载时复核名称、空资源、整数区间、maxLength、默认值和约束。同步数据库、CLI、阶段路线及索引，新增本记录与验证报告。无文件删除、Git 提交或用户业务数据库迁移。

## 接口与边界

新增内部 Database、DatabaseInfo、ListQuery/ListResult、FieldFilter、DatabaseError；新增 schema_model、preview、apply_connection、history/history_path 和序列化迁移计划/结果。缺失数据库的预览和历史不创建路径；失败迁移不替换模型，确认绑定目标路径及版本。

数据库尚未暴露为正式 HTTP，用户权限由后续宿主接入。改名只检测和拒绝执行；大表预算、跨模块发布、自动恢复及磁盘故障注入待后续验证。

## 执行验证

Windows MSVC、Rust 1.99.1，实际 SQLite 3.54.1。统一 cargo check --workspace --offline --locked、cargo test --workspace --offline --locked、cargo clippy --workspace --all-targets --offline --locked 均退出 0；当次 145 项常规测试通过，1 项浏览器探针忽略，Clippy 如实保留 22 个未消费 CRUD 等警告。

最终迁移定向 11/11、CLI 迁移集成 2/2，均退出 0；新增 48 数据库压力测试另行通过，累计 146 个不同常规测试已有成功记录。保留一次未稳定复现的 Busy 及夹具唯一性修复事实，不将可能原因写成确证。详细命令、覆盖和限制见 [阶段 04 验证](../verification/0006-sqlite-runtime.md)。后续进行正式 HTTP 认证、授权、并发与错误处理验收。
