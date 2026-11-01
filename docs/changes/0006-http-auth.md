# Rust HTTP、管理员认证与受控资源 API

状态：已执行；日期：2026-11-01；前置：[SQLite Runtime 与迁移](0005-sqlite-runtime.md)。

## 背景与方案

将阶段 04 的真实 SQLite Resource Runtime 接入 Rust HTTP，按照同一 Application IR 执行权限、字段验证和页面描述投影。认证采用本机初始化账号、Argon2id、安全 Session Cookie 和 CSRF；保持 KEngine 通用边界，不引入新的自有 Crate。

## 实施文件

新增 `crates/kenvoy/src/http/mod.rs`、`handlers.rs`、`projection.rs`、`tests.rs`：统一路由、脱敏错误、请求边界、单策略权限、同代状态与取消安全发布、真实文件适配和 loopback 测试。

新增 `crates/kenvoy/src/auth.rs`、`database/auth.rs` 及认证定向测试：密码处理、Session 摘要、轮换事务、容量控制、固定认证 Schema 漂移检查。修改 `database/mod.rs`、`worker.rs`，只通过专属连接线程的固定操作访问认证表，不将 Connection 交给 HTTP、用户 JavaScript 或客户端。

修改 `main.rs`、`project.rs`、`error.rs`：本机 `admin init/add-user`、隐藏密码/显式 stdin、最小真实 `dev`、public_origin 校验、结构化日志与 HTTP/数据库关闭顺序。新增 `tests/admin_cli.rs` 和 `tools/tests/http-cli.mjs`，验证真实 Binary 与真实网络请求。

修改 `validation.rs`：JSON 任意深度重复键拒绝，保留标准解析器限制。修改 `database/query.rs`、`worker_tests.rs`，搜索及 Contains 拒绝 NUL，避免 SQLite LIKE 模式截断。

修改根及 Kenvoy Cargo manifests/lock：新增 argon2 0.6.0、getrandom 0.4.3、hmac 0.13.0、rpassword 7.5.4、tracing 0.1.44、tracing-subscriber 0.3.23（env-filter/json）、url 2.5.8，21 个新增锁定包；Axum/Tower/tower-http 从已有探针进入正式依赖。来源 crates.io，位置为已有 Cargo 缓存，未新增 Workspace 成员。

同步 security-design、runtime-design、cli-design、项目配置与阶段索引，新增本记录与独立验证报告。无文件删除、Git 提交、对外发布或用户业务数据迁移。

## 接口与行为

新增内部 `HttpRuntime::new/router/serve/publish`、`Auth`、`SessionView/SessionResponse` 和独立错误类型。资源 API 使用 `/api/{resource}` 与 `/{id}`，认证/元数据使用 `/_kenvoy/*`，前端资产只能来自显式 client 根。请求不解析 TSX、不重复初始化数据库。

POST/PATCH/详情返回 `{data}`，列表返回 `{data,page,page_size,total,generation}`；错误包含 code/message/request_id，字段错误另有 fields。公开 Create 按单条表单策略匹配字段，匿名不会因此获得其他 CRUD 权限。未实现的前端不会被替换为假 HTML；未知 API 不返回 SPA shell。

发布在写锁内串行执行安全迁移并替换同代模型/资产，调用者取消不会取消已接受发布。跨进程崩溃恢复、开发文件监听、生产产物与浏览器体验仍由后续阶段完成。

## 执行验证

Windows、Rust 1.99.1、SQLite 3.54.1。统一 `cargo check --workspace --offline --locked`、`cargo test --workspace --offline --locked`、`cargo clippy --workspace --all-targets --offline --locked`、`cargo fmt --all -- --check` 均退出 0；当次 168 项常规测试通过，1 项既有浏览器探针忽略。普通 Clippy 保留发布接口未被生产生命周期消费的三项警告；严格 `-D warnings` 命令因此退出 1，没有压制。

HTTP 定向最后十项全部通过；管理员 CLI 两项通过；Node 真实 CLI/HTTP 脚本二十项断言通过，重启持久化成立。Node 的 Windows 强制停止不等于优雅退出，后者由 Rust loopback 测试独立覆盖。查询 NUL 两断言补入既有测试后定向 1/1 通过。初始 HTTP 两项失败、修复原因、命令及剩余限制完整保留在 [阶段 05 验证](../verification/0007-http-auth.md)。

后续验证真实 React/Admin 浏览器闭环、中文表单引用传输、静态资产构建与原子切换、开发更新、生产运行和 JavaScript 扩展；不将本阶段验收描述为最终产品完成。
