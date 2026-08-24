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
