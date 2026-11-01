# 阶段 05：Rust HTTP、账户与请求安全验证

状态：已执行；日期：2026-11-01；前置：[SQLite 与迁移](0006-sqlite-runtime.md)。

## 环境与验收范围

Windows、PowerShell、Rust 1.99.1、MSVC；严格双 Crate 工作区。真实 SQLite 3.54.1 保持独立数据目录，测试使用各自临时项目和数据库。HTTP 使用 Axum 0.8.9、Tokio 1.54.1、Tower 0.5.3、tower-http 0.7.1；没有引入其他后端运行时。

本阶段完成通用资源 API、基础管理员/普通用户账户、服务端 Session、CSRF、权限检查、客户端描述投影、真实静态文件适配和最小 dev 服务生命周期。React/Admin 页面、构建发布、文件监听、Server Function 与正式 run 尚未由本阶段完成；没有用静态页面或模拟 API 代替这些能力。

## 依赖与接口核对

新增必要依赖 argon2 0.6.0、getrandom 0.4.3、hmac 0.13.0、rpassword 7.5.4、tracing 0.1.44、tracing-subscriber 0.3.23（env-filter/json）、url 2.5.8；已有 Axum/Tower/tower-http 转为正式宿主依赖。来源 crates.io，下载至现有 Cargo 缓存，版本由 Cargo.lock 固定；本次 fetch 成功，新增 21 个锁定包，无额外 Workspace 成员。依据本项目已有必要工具/依赖授权执行。

Argon2 0.6 的 `hash_password_with_salt` 使用字节 salt，验证使用该版 PHC 接口；未照搬 0.5 API。接口核对参考 [官方 Argon2 API](https://docs.rs/argon2/0.6.0/argon2/struct.Argon2.html)，最终以实际 Rust 编译与密码测试为证据。采用 Argon2id v19、19456 KiB、2 次、并行度 1、32 字节输出、16 字节随机 salt。没有进行生产容量或密码工作因子的负载基准测试。

## 实际命令与结果

下列统一门禁由主协调执行；定向 HTTP、查询与格式命令由对应实现任务执行。表内记录观察到的结果，不把后续新增断言倒计入先前的全量命令。

| 命令 | 实际结果 |
|---|---|
| `cargo fetch` | 退出 0；本次新增 21 个锁定包 |
| `cargo check --workspace --offline --locked` | 退出 0 |
| `cargo test --workspace --offline --locked` | 退出 0；当次 168 项常规测试通过，1 项既有跨工具浏览器探针忽略 |
| `cargo clippy --workspace --all-targets --offline --locked` | 退出 0；保留 3 条发布接口尚无生产消费者的 dead_code 警告 |
| `cargo fmt --all -- --check` | 退出 0 |
| `cargo test -p kenvoy --bin kenvoy http:: --offline --locked` | 最后阶段 05 定向复验退出 0，10/10 通过 |
| `cargo test -p kenvoy --test admin_cli --offline --locked` | 退出 0，2/2 真实 Binary 测试通过 |
| `node tools/tests/http-cli.mjs` | 退出 0，20 项真实 HTTP 断言通过 |
| `cargo clippy -p kenvoy --all-targets --offline --locked -- -D warnings` | 退出 1；仅 HttpRuntime.publish、SetupError::Publication、Database.replace_application 的未消费链导致失败，未添加 allow/expect 压制 |
| `cargo test -p kenvoy --bin kenvoy queries_escape_like_literals_validate_types_and_use_stable_pagination --offline --locked` | 退出 0，1/1；统一门禁后补充 search/Contains 拒绝 NUL 的两项断言 |
| `cargo test -p kenvoy --bin kenvoy unicode_page_form_ids_use_one_strict_percent_decoding_pass --offline --locked` | 后续联调定向复验退出 0，1/1；中文路由提交成功，非法 UTF-8 与二次编码不能匹配表单策略 |
| `node tools/verify-docs.mjs` | 本次文档落盘后退出 0；46 个 Markdown、170 条本地链接通过 |

若 Cargo 输出未能完成 incremental 目录重命名、Windows 拒绝访问的 note，本轮对应命令仍按其实际退出码记录；该提示影响缓存复用，不能据此宣称源码测试失败或已经修复环境原因。

## 通过的关键行为

- 公开表单真实写入 SQLite；匿名不能列表、读取详情、修改或删除，普通用户不能调用管理员默认 CRUD。管理员能够列表、更新、删除，重启后数据与 Session 保留。
- 公开提交按一个表单策略完整匹配字段，不能把多个表单的字段合并授权。响应仅回显允许字段及系统 ID/时间，隐藏字段默认值不泄漏。
- 登录和退出轮换会话，旧 token 失效；Cookie 为 HttpOnly/SameSite=Lax，HTTPS origin 增加 Secure。元数据不含源码位置、文件路径、PHC、Session 原文或服务端 handler 实现。
- Host、Origin、CSRF、重复安全头/Cookie/JSON/查询参数、请求体大小、过时代际、非法 ID/字段/排序及内部表访问均有失败回归。
- 新匿名会话、登录与公开 Create 的限流达到上限后返回受控错误；计数表有界。角色检查由服务端执行。
- 人为删除测试独占数据库的 customers 表，真实读取返回脱敏 500；响应不包含 SQL、数据库路径或驱动错误。
- 无前端产物时页面明确返回 404；有真实文件时只服务注册页面及 client/assets。匿名管理导航跳转登录，API 401 不跳转 HTML；遍历路径和项目私有文件不可访问。
- 发布调用方被取消后，已接受任务继续持有模型写锁，完成数据库与 HTTP 同代发布；破坏性迁移失败保留旧模型和数据。提交后没有可失败的第二次数据库查询。
- Rust 真实 loopback 测试经过 TCP 请求并显式触发优雅退出。Node 脚本在 Windows 使用进程强制停止验证持久化，明确不把它当作 Ctrl+C/优雅退出验证。

## 发现与修正

初次 HTTP 四项测试中两项失败：一项将字符串 `source` 与 `resources` 误匹配，改为检查具体 JSON 属性；另一项证明嵌套 JSON 重复键仍被 serde_json::Value 覆盖。后者改为递归对象/数组访问器，在任意深度查重，仍由 serde_json 负责标准语法与递归深度限制。修复后四项通过，扩展后十项通过。

独立审查发现发布成功后再查询 database.info 可能因队列满而留下旧 HTTP 模型，改为提交前校验新代际、提交后同步发布；任务由宿主跟踪，调用者取消不打断已接受迁移。另修复 session 首次查询有效、第二次查询失效时绕过新建限流的竞态：复用首次有效视图，否则先计额度再创建新匿名上下文。

查询输入后续补充 NUL 拒绝，防止 SQLite LIKE 截断查询模式。前端接入期间另发现 Unicode 路由的表单 ID 不能直接放入 Fetch Header，改为客户端 encodeURIComponent、HTTP 严格解码一次并匹配策略。新增一项定向测试通过；首次执行被尚未准备发行资产的阶段 06 build script 阻断（退出 1，未进入测试），协调任务将未就绪构建脚本暂留工具目录后重跑通过。该后续定向验证不计入上述阶段 05 十项结果，也不改写已执行 168 项全量命令的统计。

## 限制与后续建议

发布接口已由定向测试消费，watcher 尚未接入，因此严格 `-D warnings` 门禁未通过；普通 Clippy 退出 0 的事实与警告同时保留。后续应由真实开发生命周期消费接口，而非关闭检查或添加无意义调用。

尚未完成真实浏览器表单/Admin、安全渲染、Fast Refresh、生产 build/run 和 JS Runtime 的最终项目闭环。没有 HTTPS listener/证书部署实测：当前 HTTPS origin 表示由部署代理终止 TLS，Axum listener 本身为 HTTP。限流为单进程固定窗口，不信任 Forwarded；多实例、代理来源策略、配置化额度、行级权限、MFA 和账号找回待后续设计。

本轮没有修改用户业务数据库、发布服务、提交或推送 Git。现状契约见 [安全设计](../security-design.md)、[Runtime](../runtime-design.md)、[CLI](../cli-design.md)，实施文件清单见 [阶段 05 变更](../changes/0006-http-auth.md)。
