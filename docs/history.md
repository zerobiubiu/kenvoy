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

### 执行验证记录（2026-08-25，邮箱重置 + 0.1 标签准备）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`、`cargo publish -p kengine --dry-run --allow-dirty`、`cargo package -p kengine --allow-dirty --no-verify`
- 结果：全部通过（80 个测试、4 个编译路径）
- 实现：
  1. **邮箱重置密码链路**：
     - DB 新增 `kv_password_reset` 表（user_id + sha256 hash + expires_at + used_at，1 小时过期）
     - `UserStore::create_reset_token`（生成 uuid×2 + sha256 hash 存库）/ `consume_reset_token`（一次性消费，标记 used_at）/ `reset_password`（argon2 重新哈希）/ `user_id_by_username`（重置流程需要 user_id）
     - 端点 `POST /__kenvoy/auth/forgot` body `{"username":"..."}`：总是返回 200 不泄露账号存在性；生成 token 后输出 eprintln（含明文 token，仅 dev 模式供调试）
     - 端点 `POST /__kenvoy/auth/reset` body `{"token":"...","new_password":"..."}`：复用 `password_policy` 强度校验；token 无效/已用/过期返回 400
     - `runtime.js doforgot` 真实调端点；dev mode 响应体含 `devToken` 供 UI 展示（生产邮件发送链路在注释中提示）
     - 依赖 `sha2 = "0.10"`（feature db）
  2. **Tabler 图标全量**：`icons.css` 从 45 个子集扩到 4962 个（完整 `@tabler/icons-webfont@2.47.0` 码点集）；生成脚本 `gen_icons.py`（一次性）已生成
  3. **README 文档化**：`workspace` 结构 + `Features` 表 + 裁剪示例（`--no-default-features` 三种组合）；License 段位于最上
  4. **0.1.0 标签**：`git tag -a v0.1.0` 已创建（`406f2e0` 上方 `afd2e2e`）；未推远端（待用户推）
- 发布准备：
  - `cargo package -p kengine --allow-dirty --no-verify` 成功（31 文件，1.6 MiB，含全部图标资产）
  - `cargo publish -p kengine --dry-run --allow-dirty` 成功（需 `cargo login` 后实际发布）
  - 干道顺序：`cargo publish -p kengine` → `cargo publish -p kenvoy`（kenvoy 依赖 kengine）
- 冒烟（真实 PG）：forgot 200（devToken 返回）→ reset 200（一次性）→ 再次 reset 400（token 已用）→ 新密码登录 200
- 已知限制：邮件发送链路未集成（dev mode 借 eprintln + devToken 调试）；`forgot` 当前不验证邮箱存在性（按"不泄露账号"设计）；`password_policy.validate` 第二个参数（username）当前传空串（计划接 full name 后改）
- 验证结论：✅ 全部通过；准备 v0.1.0 发布 crates.io

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

### 执行验证记录（2026-08-25，User/Department 字段下拉 + auth mode 下发 + 审批流客户端）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（kengine 52 + kenvoy 25 + db_pg 5 + doctest 1 = 83 个测试、4 个编译路径）
- 实现：
  1. **User/Department 字段下拉**：`FieldType::User` / `::Department` 渲染为受控 `select[data-source="users|departments"]` + `<option value="">加载中…</option>` 占位；`runtime.js` 加载时按 `data-source` 拉 `/__kenvoy/auth/options/{users,departments}` 填充选项（`label` 按 `data-type` 选 `displayName` 或 `name`）
  2. **公共选项端点**：`GET /__kenvoy/auth/options/users`（仅需登录、返回 `[{id, username, displayName}]`）、`/options/departments`（返回 `[{id, name, parentId}]`）用于表单下拉，不要求管理员
  3. **auth mode 下发到 runtime**：`dev::page_or_static` / `build_cmd` 注入 `<body data-auth-mode="open|review|hr">`；`runtime.js ensureLoginBox` 读取后隐藏 hr 模式的「注册账号」按钮（仅 `open` / `review` 模式可见，与服务端策略一致）
  4. **审批流客户端按钮**：`runtime.js` 监听 `[data-kv-flow]` 点击（`action` = `start` / `withdraw` / `approve` / `reject` / `add-sign`，`data-flow` / `data-record-id` / `data-node` / `data-confirm`）；调用对应端点、失败提示、成功后 `location.reload`
  5. **dev.rs 重构**：`run` 主体只做 wiring，路由拼装抽到 `build_router`（clippy `too_many_lines` 过线）
  6. **body limit 分层**：`dev` 1MB 全局 / auth 64KB / `PUT data/{id}` 1MB / `upload_file` 走 `upload_body_limit(storage)`
  7. **AppState.auth**：`AppState` 新增 `auth: AuthConfig` 字段供渲染时读取
- 修正：
  - `form_fields_render_by_type` 断言从 `value="{{currentUser.name}}"` 改为 `data-source="users"` / `data-type="user"` / `加载中…`（user 字段不再走 text input）
  - clippy `match_wildcard_for_single_variants`：render_field `_` 收窄为 `FieldType::Text`
  - clippy `too_many_lines`：dev::run 拆为 `run` + `build_router`
  - clippy `collapsible_if`：inject_body_auth_mode 改 let-chain
- 补充单测：`inject_body_auth_mode_inserts_data_attr` / `_preserves_existing_body_attrs` / `_passthrough_without_body`
- 变更说明：v0.1.0 tag 仍指向 `fb3faf6`（SMTP 集成后）；本次变更需作 v0.1.1 / v0.2 增量（是否需重发由用户决定）
- 已知限制：`packages/` 外部库依赖闭源平台分发暂缓（未变更）

### 执行验证记录（2026-08-25，Tabler 页面骨架 + Button + StatCard icon + Section kind=navbar）

- 命令：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（kengine 56 + kenvoy 25 + db_pg 5 + doctest 1 = 87 个测试、4 个编译路径）
- 实现：
  1. **页面骨架**：`render_page_with_prefix` 在 `<body>` 下加 `<div class="page"><div class="page-wrapper"><div class="page-body"><div class="container-xl">…</div></div></div></div>`（Tabler 官方推荐结构）
  2. **Section card-header 分组**：`render_section` 拆为独立函数，`<Section title="...">` 改为 `<div class="card-header"><h3 class="card-title">…</h3></div><div class="card-body">…</div>`（双容器·可视分隔）
  3. **Section kind="navbar"**：新增 IR 字段 `SectionProps.kind`，parser 读取 `kind` 属性；`render_section` 遇 `kind="navbar"` 渲染为 `<nav class="navbar navbar-expand-md bg-white border-bottom"><a class="navbar-brand" href="/">title</a>…</nav>`，brand 来自 title、子元素进 `.navbar-nav` （可放导航项 / `ti ti-*` 图标）
  4. **Button 节点**：
     - IR：`Node::Button { props: ButtonProps }`（`value` / `kind` / `size` / `icon` / `action` / `href` / `i18n` / `disabled` / `extra`）
     - `action` 协议：`submit`（默认，交所在 `<form>` 提交）/ `navigate`（跳转 `href`）/ `flow`（交给 runtime.js `[data-kv-flow]`）
     - `extra: BTreeMap<String, String>`：parser 收集未声明属性（`data-*` / `aria-*` / 自定义），render 时原样透传——保证 `data-flow` / `data-record-id` 等审批属性不丢
     - render：`<button class="btn btn-{kind}[ btn-{size}]" [disabled] data-kv-action="…"[ data-href="…"][ {extra}]><i class="…"></i> text</button>`
  5. **StatCard title/icon**：IR `StatCardProps` 加 `title` / `icon` 字段；render 输出 `bg-{color}-lt avatar` 图标圈 + `display-6` 主数 + 单独 `kv-stat-title` 标题
  6. **示例项目升级**：`examples/leave-app/apps/layout.kenvoy.xml` Root 改用 `kind="navbar"` Section + Row 包裹 Slot + Section 页脚；`apply/page.kenvoy.xml` 表单底部加主提交/保存草稿/返回 三个 Button（`ti ti-send` / `ti ti-device-floppy`）；`list/page.kenvoy.xml` 加 3 个 StatCard（待审批/已批准/已驳回，橙/绿/红三色图标）
- 补充单测：`page_skeleton_wraps_body` / `section_renders_card_header_when_title_set` / `section_kind_navbar_renders_nav` / `button_renders_with_kind_icon_action`
- clippy 修正：`render_node` `too_many_lines`（Section 拆为 `render_section`）/ `format-push-string`（改 `write!`）/ `format-collect`（`extra` 改 for 循环 `write!`）/ `single-match-else`（navbar 用 if let 提前返回）
- 变更说明：本轮纯前塪设计质量提升（未改业务逻辑 / API 路径），v0.1.0 tag 仍有效
- 已知限制：`packages/` 暂缓（未变更）；navbar 仍包含在 `container-xl` 内（全宽变窄）——后续可加 layout-aware 重排

### 执行验证记录（2026-08-25，顶部 navbar 拆出 page-body 到 page 顶层）

- 命令：`cargo fmt --all -- --check`、`cargo test --workspace`（经 `KENVOY_TEST_DATABASE_URL`）、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（kengine 57 + kenvoy 25 + db_pg 5 + doctest 1 = 89 个测试、4 个编译路径）
- 实现：
  1. **顶部 navbar 重排**：`render_body` 返回 `(top_nav: Option<String>, body: String)`；`render_page_with_prefix` 将 `top_nav` 直接拼到 `<div class="page">` 顶层（page-body / page-wrapper 之外），保持 navbar 全宽背景
  2. **拆出逻辑**：`extract_top_navbar(root: &Node)` 检测最外层 layout 的 Root 首个 child 是否为 `Section kind="navbar"`，是则独立渲染为 nav HTML、返回新 Root（其余 children）供 body 包裹
  3. **layout chain 跳过最外层**：原 `iter().rev()` 包含最外层，现改为 enumerate 跳过 rev 后最后一项，避免“外层重包裹”导致 navbar 重复
  4. **render_navbar_section 独立**：`render_section` 的 navbar 分支重构为公共 `render_navbar_section(props, children, bind, depth)`（不依赖 ir，仅按 bind 做 `{{名}}` 替换），既可被 render_section 调用也可被 extract_top_navbar 调用。navbar 子项走 `render_nodes_standalone`（仅支持 Text/Button，其他节点类型忽略）
- 补充单测：`top_navbar_lifted_outside_page_body`：验证 navbar 位置在 `<div class="page">` 之后、`<div class="page-wrapper">` 之前；主体内容仍在 `page-body` 之后
- 踩坑：原 `iter().rev().skip(1)` 实际跳过了最内层（外层被复包裹）——修正为 enumerate + `i + 1 == chain_len` 检测
- 验证（dev 跑 examples/leave-app 真实产物）：`<body><div class="page"><nav class="navbar ...">...</nav><div class="page-wrapper"><div class="page-body"><div class="container-xl">...</div>...</div></div></div></body>` —— navbar 全宽、只出现一次
- 变更说明：v0.1.0 tag 仍有效（本轮纯前塪调整，不改 API/响应/路由）

### 执行验证记录（2026-08-25，示例改为完整功能模板 + 全链路冒烟）

- 命令：`cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（89 个测试、4 个编译路径）
- 实现（`examples/leave-app` 升级为完整功能模板）：
  1. **seed.sh / seed.bat 引导脚本**（幂等）：建 5 角色（管理员100/HR 80/人事总监70/部门主管50/普通员工10）+ 3 部门 + 5 演示账户（admin/hr/ceo/alice/bob，密码 demo123）+ 4 通讯录人员（含 2 名无账号人员，演示人事录入无账号场景）；支持 `--reset` / `--reset-users`
  2. **账号等级 = 角色等级**（确认既有架构）：`kv role add --level` 决定；`kv user add` 分配角色即得对应等级；`admin` 达标 `[auth].admin_level=90`
  3. **CLI `department` 别名 `dept`**（seed 脚本简写）
  4. **models.kenvoy.xml**：请假单补 `审批状态` / `当前节点` 字段（流程副作用写库）
  5. **flow.kenvoy.xml**：发起人节点移到末尾作驳回终点；首节点为部门主管审批；`OnApprove` 分支（≤3 → 人事备案；>3 → 人事总监）；HR 备案 notify 终态写入 已通过/已完成；驳回写入 已驳回
  6. **publish.kenvoy.xml**：5 角色完整权限矩阵 + 字段读写（附件/审批状态）+ DataFilter（普通员工仅本人，主管/总监/HR/管理员全量）
  7. **layout.kenvoy.xml**：navbar 含 工作台/提交请假/审批列表 导航（Button navigate）+ 登录态徽章（runtime.js）
  8. **dashboard**：未登录引导条（`data-kv-guest-hint` + `data-kv-guest-login`）+ 统计卡 + 图表 + 流程查看器
  9. **list 页**：`<Table flow="请假审批流">` 操作列（`_actions`）— runtime.js 行内 撤回/通过/驳回 按钮（data-kv-flow 协议）
  10. **runtime.js**：navbar 登录徽章（用户名 + 退出）/ 未登录「登录 / 注册」按钮；guest hint 显示/隐藏；表格行带 `data-record-id` + `_actions` 操作列渲染；表格查询选择器修复（`table[data-model]` 现在与 render 输出一致）
  11. **IR/render**：`SectionProps.extra`（data-*/id 透传）、`TableProps.flow`、table 元素本身带 `data-model`（修 wrapper 与选择器不一致）
  12. **create_data 回传 id**：创建响应 `{...payload, "id": N}`，供前端发起流程
  13. **README.md**：启动步骤 / 演示账户表 / 端到端演示步骤 / 目录说明 / 重置
- 冒烟（真实 PG + MinIO）：seed 幂等两跑无重复；5 账户登录 200；bob 提交请假单（5 天）→ start 流程 200（当前节点=部门主管审批）→ alice 审批 → 人事总监审批 → ceo 审批 → done（HR 备案自动）；记录 `审批状态=已通过`、`当前节点=已完成`
- 已知限制：`list` 页操作列按钮无角色判断（点击由后端 403 拒绝，模板演示足够）；数据库历史脏数据需手动清理（`docker exec kenvoy-pg18 psql -U kenvoy -d kenvoy -c "TRUNCATE contacts, user_roles, kv_flow_instance, sessions RESTART IDENTITY CASCADE; DELETE FROM users; DELETE FROM roles; DELETE FROM departments;"`）

### 执行验证记录（2026-08-25，模板登录/表单/审批 UI 修复 + Playwright 端到端）

- 命令：`cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（89 个测试、4 个编译路径）
- 动机：用户反馈“模板登录功能不可用”
- 用 Playwright（headless chromium）复现并逐层修复：
  1. **`setLoginMode` 引用未定义 `err`**（`err.style.display`）——脚本在注册 click listener 前抛错，登录框永不弹出。补 `var err = document.getElementById("kv-login-err")`。**根因**
  2. **`<Button action="submit">` 不提交表单**：渲染 `type="button"`，runtime 无 handler。新增 `[data-kv-action="submit"]` click → `form.requestSubmit()`（无参，避免非 submit button 参数）
  3. **按钮须在 Form 内**：模板把提交按钮移到 `<Form>` 内，否则 `closest("form")` 找不到
  4. **`{{currentUser.*}}` 绑定客户端替换**：readonly 申请人默认值 SSR 不替换。新增 `resolveCurrentUserBindings()`（name→display_name 兼容），表单提交 payload 统一替换
  5. **readonly user select（disabled）不进 FormData**：提交时强制写入选中的 user 下拉值/当前用户 id；loadOptions 完成后自动选中当前用户
  6. **字符串数字存 0**：`insert_record`/`update_record` 对 Number 字段 `as_f64()==null→0`。新增 `to_f64`（String 解析）与 `to_bool`（true/1/yes/on）
  7. **data-flow 双重编码**：表格操作列按钮属性存 `encodeURIComponent` 值，`getAttribute` 后 handler 再编码。改为存原文（escapeHtml）
  8. **审批错误响应容错**：后端返回纯文本（如“流程未启动”）时 `r.json()` 抛 SyntaxError。改为 `r.text()` + try-parse，错误/成功提示原文
  9. **create_data 已回传 id**（上一轮）；模板部门字段改可下拉选择（原 `{{currentUser.department}}` 无绑定来源）
- Playwright 端到端（headless chromium，真实 PG + MinIO）：
  - 登录：打开 dashboard 401 自动弹框 → bob/demo123 登录 → 徽章“Bob 李”、guest hint 隐藏 ✅
  - 提交：apply 页填表（含 user/部门下拉、附件为空）→ 提交 → 列表页可见新记录，请假天数正确存 2 ✅
  - 审批：bob 提交 6 天单并 start → alice 列表点“通过” → 无报错，流程流转到人事总监审批（API 验证 200 currentNode=人事总监审批）✅
- 已知限制：审批按钮无角色显隐（点击由后端校验拒绝）；列表页点“撤回/通过”对未启动流程的行会返回“流程未启动”（合理业务错误，UI 已容错提示）

### 执行验证记录（2026-08-25，系统数据源 + 管理/人事/详情应用页面）

- 命令：`cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（89 个测试、4 个编译路径）
- 实现：
  1. **系统数据源 `<Table source>`**：`TableProps.source`（`users` / `roles` / `departments` / `contacts`）；parse 校验放行（source 非空不要求模型）；render 输出 `data-source` / `data-sys`；kenvoy 新增 `GET /__kenvoy/system/{source}`（管理员，账号等级 ≥ `[auth].admin_level`），返回统一 JSON 数组
  2. **系统管理应用 `apps/admin/`**：users（含 通过/禁用 操作按钮）、roles、departments、contacts 四页；runtime 新增 `data-kv-sys="approve|disable"` 协议（POST `/__kenvoy/auth/users/{id}/approve|disable`）
  3. **人事中心 `apps/hr/`**：通讯录 + 人员台账（复用 `source` 数据源），展示无账号人员场景
  4. **module 复用**：`module/进度卡.kenvoy.xml` 全局组件（从 dashboard 私有组件提升），工作台 + 人事中心共用
  5. **详情页**：`TableProps.detail`（行点击跳转 `/leave/detail?id=N`）；`apps/leave/detail/` 页面；runtime `[data-kv-detail]` 渲染字段键值 + `data-kv-detail-flow` 拉流程实例状态（当前节点 / running/done/驳回）
  6. **全局导航**：layout navbar 增「人事中心」「系统管理」入口
  7. **README**：目录结构更新 + review 审核模式说明（管理模式 `open→review` 演示注册审核闭环）
- Playwright 冒烟（headless chromium）：admin 登录 → 4 管理页表格渲染（users 5 行 / roles 5 / departments 3 / contacts 4）；hr 页 + 进度卡组件 2 处；列表行点击 → 详情页 10 字段 + 流程状态“running · 人事总监审批”；无 JS 错误；API 层 admin 200 / bob 403
- 已知限制：系统表仅 GET（新增/编辑走 CLI / 管理 API）；详情页未做操作按钮（返回列表即可）

### 执行验证记录（2026-08-25，登录即弹框 + 应用级权限分发）

- 命令：`cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo build -p kenvoy --no-default-features`、`cargo build -p kengine --no-default-features`
- 结果：全部通过（89 个测试、4 个编译路径）
- 实现：
  1. **打开页面即弹登录**：runtime.js 页面加载末尾 `if (!getUser()) showLogin()`——不再等 401 才弹；未登录访问任何页面（含纯静态页）立即弹出登录框
  2. **会话携带角色/等级**：`setAuth` 合并 login 响应的 `level` / `roles` 到 localStorage 用户对象（原仅存 user）
  3. **前端应用级访问控制**：runtime `applyAccessControl()` 过滤 `[data-roles]`（逗号分隔角色，命中任一可见）与 `[data-min-level]` 元素；全局导航「人事中心」`data-roles="HR,人事总监,管理员"`、「系统管理」`data-min-level="90"`；render_nodes_standalone（navbar 内 Button）透传 extra 属性（原来丢弃）
  4. **SSR 页面级权限**：`page_or_static` 对 `publish.allow_roles` 非空页面——已登录但角色不符 → 403 页面（含账号名与所需角色）；未登录 → 正常渲染（由前端弹框）
  5. **会话 cookie**：登录响应加 `Set-Cookie: kv_token=…; HttpOnly; SameSite=Lax`（浏览器顶级导航不带 Bearer 头，SSR 需 cookie）；登出清 cookie；`dev::auth_user` 优先 Bearer、回退 cookie
  6. **模板 publish**：admin 四页与 hr 两页新增 `publish.kenvoy.xml`（页面级 `<Import>`），`admin/*` 仅管理员、`hr/*` 限 HR/人事总监/管理员（publish 文件须与 page 同目录并显式 Import）
- Playwright 冒烟：未登录 dashboard/admin 均立即弹框；bob 导航隐藏 人事中心/系统管理；bob 直访 /admin/users → 403“无权访问”；admin 导航可见且表格 5 行；全程无 JS 错误 PASS
- 已知限制：`admin` 应用因 HttpOnly cookie 无法被 localStorage 前端逻辑读取（导航按 localStorage 角色过滤——两者一致由登录响应填充）；页面级权限在 build 静态产物中不生效（SSR 仅 dev/build 数据快照场景）
