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
