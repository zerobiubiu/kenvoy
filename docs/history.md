# 变更历史

> 追加型历史文档：只增量追加，不重写已有历史。

## 2026-08-23 — 确立 IR 契约与 demo 最小闭环方案

- 需求摘要：为 Kenvoy 语义化低代码平台确立「XML → IR → HTML」渲染链路的设计与实现计划
- 执行结果：新增 `docs/01-ir-contract.md`（IR 契约 v0.1）与 `docs/02-demo-plan.md`（demo 实现清单）
- 影响范围：`kengine` 对外产物形态（IR JSON）、渲染器分层（`kengine-html`）、`kenvoy` CLI 的 `dev` 命令
- 验证情况：设计文档，尚未实现；B1（IR 类型）动工后在本条目下追加执行验证记录
- 关联文档：`docs/01-ir-contract.md`、`docs/02-demo-plan.md`

### 执行验证记录（2026-08-23，B1 完成）

- 环境：本地 Windows，Rust 1.98
- 命令：`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：`crates/kengine/src/ir/` 新增 `IR` 类型（`ProjectIr` / `Model` / `Layout` / `Page` / `Node` 及 props）+ serde round-trip 测试
- 决策确认：D1~D6 全部确认；`Custom` 组件因 serde 内部标签限制改用 `component` 字段承载组件名（契约已同步更新）
- 已知限制：B2（XML 解析）未开始；`flow` / `permissions` 为占位结构

### 执行验证记录（2026-08-23，B2 完成）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：`crates/kengine/src/parse/`（`build_project` / `ParseError` / `SourceFile`）；XML 解析子集：`<Models>` / `<Layout>` / `<Page>`，组件树 `Row/Col/Section/Slot/Form/Field/Table/Column/StatCard/Text` + 未知元素→`Custom`；layout 逐级匹配与 `layoutChain` 解析；模型/字段引用校验
- 依赖：kengine 新增 `roxmltree` + `thiserror`
- 结构变更：`ProjectIr.models` / `.layouts` 及 `Custom.props` 由 `HashMap` 改为 `BTreeMap`（序列化输出确定，契约已同步）
- 测试：golden 测试 `tests/parse.rs` + `tests/fixtures/leave/`（5 个 XML → `expected.json`）
- 已知限制：`flow` / `publish` / `<Validation>` 尚未解析；`filter` / `sort` 未从 XML 解析（IR 字段保留）

### 执行验证记录（2026-08-23，B3 完成）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine-html 8 个单元测试）
- 实现：新增 `crates/kengine-html`，`render_page(&ProjectIr, &Page) -> String`（纯函数无 I/O）；布局链嵌套 + `Slot` 替换；Form 按模型字段类型渲染控件（text/textarea/number/date/select/boolean/file）；Table 表头 + 空态；StatCard；未知元素（Custom）渲染为 `data-component` 容器；HTML 转义防 XSS；内置极简样式
- 决策：D2（独立 crate `kengine-html`）落地
- 已知限制：`filter` / `sort` / 分页数据未渲染（demo 空态）；客户端交互（提交/审批）未实现

### 执行验证记录（2026-08-23，B4+B5 完成）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kenvoy 9 个测试，含 watcher 热重建与 HTTP 层测试）
- 实现：`crates/kenvoy/src/dev.rs` 新增 `dev` 子命令（axum + notify + tokio）：扫描 `apps/` → `build_project` → `render_page` 实时渲染；`public/` 静态资源直出（防目录穿越）；文件变更合并重建（300ms 防抖）+ 轮询自动刷新（`/__kenvoy/version`）；示例项目 `examples/leave-app`（根布局 + 工作台 + 请假应用：申请表单 / 我的申请表格 / 数据模型）
- 决策落地：D3（axum + notify + tokio）、D4（整页刷新，轮询非 SSE/WS）
- 冒烟测试：`cargo run -p kenvoy -- dev` 启动，`/leave/apply` 返回 200 且含根布局与页面内容、`/hello.txt` 静态直出、404 正常
- 已知限制：`kenvoy.toml` 配置尚未读取；`kv init` / `kv build` / `kv deploy` 未实现

### 执行验证记录（2026-08-23，CLI 完整性：kenvoy.toml + init/build/deploy）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kenvoy 17 个测试）
- 实现：新增 `crates/kenvoy/src/config.rs`（`kenvoy.toml` 读取：`[project]` name/description、`[server]` host/port、`[build]` output/public_path）、`init.rs`（`kv init` 骨架：apps/ + 根布局 + dashboard + kenvoy.toml + .gitignore，拒绝覆盖已有项目）、`build_cmd.rs`（`kv build` 静态产物：每页 `<path>/index.html` + 索引页 + `public/` 拷贝，支持 `public_path` 子目录）；`kv deploy` 占位（需闭源 KenvoySpace）；dev 服务器支持配置 host/port/name/description（索引页显示描述）
- 冒烟测试：`kv init` 生成骨架；`kv build` 产出 dist（3 页面 + 静态资源）；`kv dev` 读取配置端口 5173 且索引页显示项目描述
- 已知限制：`kv deploy` 未实现（闭源）；`public_path` 仅影响构建产物目录，dev 服务器仍从根路径提供 `public/`

### 执行验证记录（2026-08-23，flow / publish 解析与页面引入）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine 15 个单元测试 + golden 更新）
- 实现：`IR` 新增 `Flow` / `FlowNode` / `FlowAction`（`next/back/notify/update/send_message/if`）、`Publish` / `FieldAccess` / `Role`；`Page` 新增 `flows`（引入的流程名）与 `publish`；`parse` 新增 `<Flow>` / `<Publish>` 解析与页面 `<Import>` 引入（`src` 相对页面目录解析，`ImportNotFound` / `ImportInvalid` / `UnknownImportType`）；顶层 `flows` 汇总引入流程，`permissions.roles` 合并各页面角色（按名去重）
- 测试：golden fixture 新增 `flow.kenvoy.xml` / `publish.kenvoy.xml` 并由申请页引入（3 节点审批流 + 3 角色）；`examples/leave-app` 同步
- 已知限制：XML 属性值中 `<` 需转义为 `&lt;`（XML 规范）；`DataFilter` 尚未解析；审批流引擎（状态机执行）未实现；publish 仅解析不参与渲染/鉴权

### 执行验证记录（2026-08-23，客户端交互：表单提交与表格数据）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：dev 服务器新增内存数据 API（`GET/POST /__kenvoy/data/{model}`，模型名需百分号编码）与客户端运行时 `/__kenvoy/runtime.js`（表格按 `th[data-field]` 渲染数据行、表单自动加提交按钮并 POST 后刷新）；`kengine-html` 表格列增加 `data-field` 属性；页面注入运行时脚本 + 自动刷新脚本
- 冒烟测试：`GET /__kenvoy/data/请假单` 返回 `[]`；POST 后 GET 返回记录；页面含 `runtime.js` 与 `data-field`
- 已知限制：数据为内存存储（重启丢失）；`readonly` 字段渲染为 `disabled` 不参与 FormData 提交；`kv build` 静态产物不含运行时脚本（无数据 API，表格保持空态）；客户端提交中文 body 在 git-bash curl 下需用 ASCII 或文件方式（浏览器 fetch 无此问题）

### 执行验证记录（2026-08-23，表达式引擎 + 表单校验 + 审批流状态机）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine 29 个单元测试）
- 实现：
  - `kengine::expr` 表达式引擎（递归下降）：中文标识符、点路径变量、比较/算术/逻辑、`empty()` 函数、`非空` 后缀、`{{ }}` 绑定求值（`resolve_bindings`）
  - `kengine::validation` 表单校验执行：对提交数据求值 `Validation.expr`，返回失败提示
  - `kengine::workflow` 审批流状态机：`start_node` / `step`（推进/退回/条件分支/更新模型/通知/消息），纯函数返回副作用
  - XML `<Validation>/<Rule>` 解析（`FormProps.validations`）；`<Validation>` 不混入组件树
- 测试：expr 9 个、validation 4 个、workflow 4 个；golden fixture/示例申请页加入校验规则
- 文档：新增 `docs/04-storage.md`（PG 18+ + S3）；`docs/03-project-structure.md` 增加 `auth/` 用户管理；README 路线图重排
- 已知限制：校验/状态机为纯逻辑，尚未接入 dev 数据 API 与渲染器（下一阶段接 PG 持久化时一并接入）；`auth/` 用户系统未实现

### 执行验证记录（2026-08-23，PostgreSQL 持久化）

- 环境：Docker 容器 `kenvoy-pg18`（`postgres:18`，端口 5433，`postgres://kenvoy:kenvoy@127.0.0.1:5433/kenvoy`）
- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`；PG 集成测试经 `KENVOY_TEST_DATABASE_URL` 连接容器
- 结果：全部通过（kengine-db 5 个集成测试：模型中文列 CRUD、缺字段默认值、用户登录/会话、角色等级/通讯录、storage_object 元数据）
- 实现：
  - 新增 `crates/kengine-db`（PG 18+）：`schema`（8 张系统表迁移：users / roles / user_roles / departments / contacts / sessions / storage_object）、`models`（IR 模型动态建表 + insert/list，中文列名）、`users`（argon2 密码、会话 token、角色等级、通讯录含无账号人员）、`storage`（S3 元数据）
  - `kenvoy` 接入：`kenvoy.toml` 增 `[database].url`；dev 数据 API 切 PG（连接失败回退内存模式）；POST 时执行表单校验（`<Validation>`，失败 400）
  - 修复：handler 持锁跨 await 导致 Future 非 Send（clone 模型定义后释放锁）；校验对空值比较错误跳过（如 `结束日期 >= 开始日期` 字段未填时不阻断提交）
- 冒烟：非法提交 400（只报对应错误）；合法提交 200 入库；重启服务器后数据保留（PG 持久化）
- 已知限制：dev 服务器若未配置数据库连接回退内存模式；模型字段类型映射简化（date/select/user 等存 TEXT）；用户/通讯录/鉴权 API 尚未暴露 HTTP 端点（下一阶段 auth）

### 执行验证记录（2026-08-23，auth 系统：HTTP API + CLI）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`；PG 集成测试经 `KENVOY_TEST_DATABASE_URL`
- 结果：全部通过
- 实现：
  - `kengine-db` 扩展：`UserDetail`（含角色名+账号等级）、`user_roles` / `list_users` / `delete_session` / `list_departments` / `link_contact`（通讯录-账号关联）
  - `kenvoy/src/auth.rs`：`/__kenvoy/auth/*` HTTP API（register / login / logout / me / users / roles+等级 / departments / contacts+关联）；Bearer token 会话
  - `kenvoy/src/cli_auth.rs` + 子命令：`kv user add|list`、`kv role add|list`、`kv contact add|list`、`kv department add|list`（读 `kenvoy.toml [database].url`）
- 冒烟（真实 PG 容器）：HTTP 注册/登录/分配角色后 `me` 显示等级 50；通讯录关联账号、无账号人员；登出后会话失效（401）；CLI 全流程（部门/角色/用户含角色分配/通讯录）
- 已知限制：HTTP auth 端点尚未做权限分级（任何能连 dev 服务的人可注册/管理）；页面级/字段级鉴权与 `DataFilter` 未实现（下一阶段）；`register` 未做密码强度/账号唯一性业务校验（DB 唯一约束兜底）

### 执行验证记录（2026-08-23，Tabler 组件 + 移动端 + 静态构建增强）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：
  - 引入 Tabler v1.3.0（MIT）：`crates/kengine-html/assets/tabler.min.css`（526KB，含 CSS 变量）；`kengine-html` 导出 `TABLER_CSS` / `RUNTIME_JS` 常量（`include_str!`）
  - 渲染类名切换为 Tabler/Bootstrap：`row` / `col-md-{span}`（移动端全宽、md 断点分栏）、`card`+`card-body`+`card-title`、`form-control` / `form-select` / `form-check-input`、`table table-hover` + `table-responsive`（移动端横向滚动）、`btn btn-primary`（运行时提交按钮）；保留 `kv-*` 语义类与 `data-*` 属性
  - 运行时 JS 从 `dev.rs` 提取为 `assets/runtime.js`（dev 与 build 共用），表单提交失败时弹出校验错误
  - dev：新增 `/assets/tabler.min.css` 路由；build：输出 `assets/tabler.min.css` / `__kenvoy/runtime.js` / `__kenvoy/ir.json`，页面注入运行时脚本
- 冒烟：页面含 Tabler 类名与栅格、CSS 200、build 产物完整
- 已知限制：Tabler 图标字体未引入（图标显示为占位）；`public_path` 前缀未作用于 CSS/runtime 引用（build 默认根路径）；具体业务组件（日期范围、上传、图表、流程查看器）与 DSL 级自定义组件未实现

### 执行验证记录（2026-08-23，数据访问鉴权：DataFilter + 字段写权限）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`；PG 集成测试经 `KENVOY_TEST_DATABASE_URL`
- 结果：全部通过
- 实现：
  - `IR`：`Publish` 新增 `data_filters`（`DataFilterRule` / `RoleFilter` / `EqCondition`）；解析 `<DataFilter><Rule model><Filter role><Eq/All>`
  - `kengine-db`：`list_records_where`（AND 相等行级过滤）；`ensure_model_table` 支持列迁移（`ADD COLUMN IF NOT EXISTS`，模型加字段后已有表自动补列）
  - `dev` 数据 API：要求 Bearer token 登录（401）；`GET` 按当前用户角色解析 `DataFilter`（无规则→全量、无匹配角色→403、`<All/>`→全量、否则 `{{currentUser.*}}` 绑定后 AND 过滤）；`POST` 校验字段写权限（`publish.fields.write_roles`，无权限→403）；`currentUser.*` 表达式上下文（id/username/displayName/email）
- 冒烟（真实 PG）：无 token 401；普通员工仅见自己的记录；管理员全量；写受限字段 403；正常写入 200
- 已知限制：多角色命中 `DataFilter` 时取首个匹配条件（未做 OR 合并）；字段读权限未在 SSR 渲染层应用（仅写入侧校验）；`DataFilter` 仅支持相等条件

### 执行验证记录（2026-08-25，S3 文件存储收尾：配置 + 上传/下载端点 + 运行时）

- 环境：本地 Windows，Rust 1.98；Docker 容器 `kenvoy-minio`（MinIO，127.0.0.1:9000）与 `kenvoy-pg18`（PostgreSQL 18，127.0.0.1:5433）
- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine-s3 3 个 MinIO 集成测试；kenvoy 19 个测试含未配置存储 503 用例）
- 实现：
  - `kenvoy.toml` 新增 `[storage]` 段（endpoint/region/bucket/access_key/secret_key），`config.rs` 读取
  - `dev` 新增 `POST /__kenvoy/file`（multipart 上传：登录鉴权 → S3 put → `storage_object` 元数据，元数据失败回滚删对象）与 `GET /__kenvoy/file/{id}`（登录鉴权 → 元数据 → S3 get，按存储 Content-Type 返回，未知 ID 404）
  - `kengine-s3`：`put` 返回 ETag（取响应头）；暴露 `bucket_name()`
  - `runtime.js`：`<input type="file">` 选中文件时先上传 `/__kenvoy/file`，payload 中写入文件 ID（失败 alert 中断提交）
  - 示例项目 `examples/leave-app/kenvoy.toml` 增加本地 MinIO 配置
- 冒烟（真实 PG + MinIO，`--noproxy` 绕过本地代理）：注册/登录 → 上传 200（返回 id+objectKey）→ 下载 200 且内容与 Content-Type 一致 → 无 token 401 → 未知 ID 404 → 重启 dev 服务器后同 ID 仍可下载（元数据在 PG、文件在 MinIO，持久化成立）
- 已知限制：下载不设 Content-Disposition（文件名由前端处理）；无大小上限/类型白名单；`storage_object.bucket` 记录实际桶名，下载固定走默认桶；`public/` 不可运行时写入的约束不变

### 执行验证记录（2026-08-25，自定义组件：inline `<Component>` + `module/` 目录）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine 35 个单元测试 + golden 更新；kengine-html 12 个渲染测试）
- 实现：
  - `IR`：新增 `ComponentDef`，`ProjectIr.components` 按名索引
  - `parse`：`module/` 目录（全局 `module/` 与 `apps/<应用>/module/`）文件解析（`Role::Module`，根元素必须为 `<Component name>`，否则 `InvalidModuleRoot`）；页面内联 `<Component>` 与 `<Content>` 平级注册；重名 `DuplicateComponent`；组件定义体参与模型/字段引用校验
  - `kengine-html`：`Custom` 节点命中定义时展开（实例 props 替换 `{{名}}` 绑定，实例子元素注入 `<Slot/>`，嵌套深度上限 8 防循环引用）；未定义组件保持 `data-component` 兜底
  - `kenvoy dev`：源文件收集扩展至全局 `module/` 目录
- 冒烟：dashboard 页面 inline `进度卡` 组件（`{{title}}` 绑定 + 插槽）渲染正确
- 已知限制：绑定仅作用于渲染时的字符串属性（Text/标题/StatCard/Field 默认值等），字段可见性等逻辑属性不支持绑定；`module/` 暂不支持通过 `<Import>` 声明式引用（按目录自动收集）

### 执行验证记录（2026-08-25，auth 注册模式 + 密码策略 + 数据鉴权加强 + 部署与上传增强）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（kengine 40、kengine-html 12、kenvoy 21、kengine-db 5、kengine-s3 3 个测试）
- 实现：
  1. **注册模式可配置**（`[auth].mode`）：`open` 公开注册（默认）/ `review` 审核注册（账号 `pending`，`POST /users/{id}/approve` 通过后方可登录）/ `hr` 人事绑定式（关闭公开注册，账号由人事/管理员经 CLI 或管理 API 创建后发放）；管理端点（用户/角色/部门/通讯录、审核）要求登录
  2. **密码强度策略**（`[auth.password]`）：`none`（默认）/ `weak` / `medium` / `strong` / `custom`（最短长度、大小写、数字、符号、不得含用户名）；HTTP 注册与 CLI `user add` 一致校验，失败 400/错误提示
  3. **DataFilter 加强**：条件支持比较操作符（`<Eq/>` `<Ne/>` `<Gt/>` `<Ge/>` `<Lt/>` `<Le/>`，IR `OpCondition`/`FilterOp`）；多角色命中按 **OR 合并**（组内 AND，不再取首条）；SQL 侧 `list_records_filtered`（组间 OR + 操作符）
  4. **字段读权限**：数据 API 响应按当前角色裁剪（`publish.fields.read_roles` 声明且角色不在 → 字段剔除，保留 id/created_at）；SSR 层无用户上下文保持全列（设计取舍：读权限在数据层生效）
  5. **Tabler 图标**：引入 Tabler Icons v2.47 字体（woff2 778KB + 45 个常用 `.ti-*` 子集 `icons.css`），dev 与 build 均输出，页面自动引用
  6. **public_path 前缀**：`[build].public_path` 非空时全部产物（页面/索引/assets/__kenvoy/public）统一置于 `dist{前缀}/`，页面资源引用与索引链接带前缀（子路径部署）；缺省行为不变
  7. **上传限制**（`[storage]`）：`max_size_mb`（超限 413）、`allowed_types`（支持 `image/*` 通配，不符 415）；`kv init` 骨架与示例 `kenvoy.toml` 附注释模板
- 冒烟（真实 PG + MinIO）：open 注册 200；strong 弱密码 400（列出全部缺失项）；review 注册 pending → 登录 401 → 管理 token approve 204 → 登录 200；管理端点无 token 401；hr 注册 403；上传 txt 415、1.5MB 413；dev 图标资源 200；`public_path=/myapps` build 产物与引用全部带前缀
- 已知限制：管理端点仅要求登录（未按账号等级分级，后续可加 level 门槛）；axum multipart 默认 2MB 字段上限（超过直接 400，未配置化）；`kv deploy` 保持占位（闭源 KenvoySpace）

### 执行验证记录（2026-08-25，遗留限制收尾：管理等级门槛 + multipart 上限配置化）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过
- 实现：
  - **管理端点等级分级**：`[auth].admin_level`（缺省 90）；管理端点（用户/角色/部门/通讯录、审核）要求登录且账号等级 ≥ 门槛，不足返回 403 并提示所需等级（`require_admin`）；CLI 管理命令不受限（负责 bootstrap 管理员）
  - **multipart 上限配置化**：配置 `[storage].max_size_mb` 时表单解析上限 = 上限 + 2MB 缓冲（业务层 413 语义优先）；未配置时默认 512MB（不再有 2MB 隐藏瓶颈，限制决策归业务层）
- 冒烟（真实 PG + MinIO）：level 0 用户访问管理端点 403、level 90 管理员 200；未配置上传限制时 3MB 文件上传 200（此前 2MB 即被 axum 默认上限拦为 400）
- 验证结论：✅ 通过；`kv deploy` 保持占位（闭源 KenvoySpace）

### 执行验证记录（2026-08-25，登录流程 + 审批流运行时 + 表格查询 + 图表/i18n/静态快照）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`
- 结果：全部通过（83 个测试）
- 实现：
  1. **前端登录流程**：`runtime.js` 全重写——统一 `kvFetch`（自动附带 Bearer token、401 时弹登录模态框、登录成功重试原请求）、localStorage 会话、登出口（页面加载可点）；表格/表单/上传统一走 kvFetch
  2. **数据 API 增删**：`PUT/DELETE /__kenvoy/data/{model}/{id}`（登录、字段写权限、表单校验与创建一致）
  3. **审批流运行时**：`kv_flow_instance` 表；`POST /flow/{flow}/records/{id}/start|step` + `GET` 状态查询；审批人校验（`approver` 为角色名，可带 `角色.` 前缀，不匹配 403）；`<If>` 条件分支由状态机求值；知会节点自动推进；副作用（`<Update>` 模型字段落库、通知/消息回显）
  4. **表格查询**：`<Filter field value>` / `<Sort field order>` 解析（表层元数据，不参与组件树）；表格输出 `data-filter/data-sort/data-page-size`；`GET /__kenvoy/data` 支持 `filter`（`f:v,f2:v2`，并入权限条件 AND）/`sort`/`pageSize`（仅模型字段，`id` 特例）；File 类型列输出 `data-type="file"` 渲染下载链接
  5. **统计图表**：`<Chart model field stat>` 内置组件；`GET /__kenvoy/stats/{model}?group=&stat=count|sum|avg`（分组统计）；运行时渲染 SVG 柱状图
  6. **流程查看器**：`<FlowViewer flow>` 内置组件，SSR 渲染节点步骤条
  7. **i18n**：`I8n/<语言>/strings.kenvoy.xml`（`<Strings><String key value>`）；`[project].locale` 指定默认语言；`<Text i18n="key">` 渲染时替换
  8. **静态站数据**：`kv build` 配置 `[database].url` 时导出 `__kenvoy/data/{模型}.json` 快照；运行时数据 API 不可用时降级读取
- 修复：`LIMIT $1` 与条件占位符冲突（编号接在条件后、bind 顺序一致）；`id`/数值/布尔字段比较值显式 `::BIGINT`/`::DOUBLE PRECISION`/`::BOOLEAN` cast（TEXT 参数与数值列比较 500）；`ensure_model_table` 系统列（created_at/updated_at）补 ALTER；`pageSize` query 字段名 rename；stats `count(*)` CAST DOUBLE PRECISION；PG 集成测试迁移并发竞态（`MIGRATE_LOCK`）
- 冒烟（真实 PG + MinIO）：登录/越权 403/审核闭环不变；流程 start → 主管审批（≤3 天自动走到知会节点并 done；>3 天条件分支到人事总监 running）；PUT 落库验证、DELETE 204；stats 返回分组统计；build 导出快照 + i18n 替换 + Chart/FlowViewer 容器 + data-sort 输出
- 已知限制：`<Filter>` 值为静态（无 `{{}}` 绑定）；`runtime.js` 登录模态框无注册/忘记密码入口（open 模式注册走 API）；审批流无撤回/加签；`kv_flow_instance` 未关联 `storage_object` 等资源清理
