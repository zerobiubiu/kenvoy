# IR 契约设计 v0.1

> 文档类型：📋 接口契约
> 状态：🔧 待执行
> 影响范围：`kengine`（解析引擎）、`kengine-html`（渲染器，待建）、`kenvoy` CLI
> 前置文档：无
> 替代文档：无
> 生效版本：kengine 0.2.0（目标）

## 1. 设计原则

1. **IR 是 kengine 的唯一对外产物**——渲染器只读 IR，永远不读 XML
2. **kengine 完成语义工作**——layout 逐级匹配、组件引用解析都在 kengine 里做，渲染器保持「笨」
3. **组件树可扩展**——内置组件用枚举，用户 DSL 组件用 `Custom` 兜底
4. **版本化**——IR 顶层带 `version`，渲染器必须校验兼容性
5. **数据绑定用 `{{ }}` 约定**——demo 阶段不做表达式引擎，渲染器只做最简替换

## 2. 顶层 IR

```json
{
  "version": "1.0",
  "project": {
    "name": "请假管理",
    "description": "企业请假管理数字化平台"
  },
  "models": {},
  "layouts": {},
  "pages": [],
  "flows": [],
  "permissions": {}
}
```

`flows` / `permissions` demo 阶段可为空数组 / 空对象，但**结构字段必须存在**（防止后期改 IR 形状）。

## 3. 实体定义

### 3.1 Model（数据模型）

```json
{
  "name": "请假单",
  "fields": [
    { "name": "申请人", "type": "user", "label": "申请人" },
    { "name": "请假天数", "type": "number", "label": "请假天数" },
    { "name": "请假类型", "type": "select", "label": "请假类型",
      "options": ["年假", "病假", "事假", "调休"] },
    { "name": "事由", "type": "textarea", "label": "事由" }
  ]
}
```

字段类型枚举（demo 子集）：`text / textarea / number / date / select / boolean / file / user / department`；完整集后续补 `money / datetime / multi-select`。

### 3.2 Layout（布局）

```json
{
  "path": "/请假管理",
  "title": "请假管理",
  "root": {
    "type": "Row",
    "children": [
      { "type": "Text", "props": { "value": "请假管理导航" } },
      { "type": "Slot" }
    ]
  }
}
```

`Slot` 是布局占位节点，渲染时被内层 layout 或页面内容替换。

### 3.3 Page（页面）—— 核心是 layoutChain

```json
{
  "path": "/请假管理/申请",
  "title": "填写请假单",
  "layoutChain": ["/", "/请假管理"],
  "content": { "type": "Row", "children": [] },
  "flows": ["请假审批流"],
  "publish": { "allowRoles": ["普通员工"], "fields": [], "roles": [] }
}
```

`layoutChain` 由 kengine **在解析期算好**（外层在前），渲染器按序嵌套渲染：根 layout 的 Slot → 下一层 layout → … → 最内层 Slot → 页面 `content`。

`flows` 为页面显式引入的审批流名（索引到顶层 `flows`）；`publish` 为页面权限配置。

### 3.4 Node（组件树）—— 统一结构

```json
{ "type": "Form",
  "props": { "model": "请假单" },
  "children": [
    { "type": "Field", "props": { "name": "申请人", "readonly": true, "default": "{{currentUser.name}}" } },
    { "type": "Field", "props": { "name": "事由", "rows": 4 } }
  ] }
```

demo 组件类型：`Row / Col / Section / Slot / Form / Field / Table / Column / StatCard / Text`，外加 `Custom` 兜底。

### 3.5 Flow（审批流）—— 页面显式引入

```json
{
  "name": "请假审批流",
  "trigger": "请假单.提交申请",
  "nodes": [
    {
      "name": "部门主管审批",
      "approver": "部门主管",
      "kind": "normal",
      "onApprove": [
        { "kind": "if", "expr": "请假天数 <= 3",
          "children": [ { "kind": "next", "target": "人事备案" } ] },
        { "kind": "notify", "users": ["申请人"] }
      ],
      "onReject": [ { "kind": "back", "target": "发起人" } ],
      "onComplete": []
    }
  ]
}
```

动作 `kind`：`next` / `back` / `notify` / `update` / `send_message` / `if`（条件分支，`expr` + `children`）。节点 `kind`：`normal` / `notify`。

### 3.6 Publish（权限）—— 页面显式引入

```json
{
  "allowRoles": ["普通员工", "部门主管"],
  "fields": [
    { "field": "附件", "readRoles": ["普通员工"], "writeRoles": ["普通员工"] }
  ],
  "roles": [
    { "name": "部门主管", "extends": "普通员工", "permissions": ["请假管理.审批.操作"] }
  ]
}
```

顶层 `permissions.roles` 为所有页面 publish 角色的合并注册表（按名去重）。

### 3.7 页面引入（Import）

`flow` / `publish` 文件只出现在 page 同级目录，且必须被页面显式引入：

```xml
<Page>
  <Import src="./flow.kenvoy.xml" as="请假审批流" />
  <Import src="./publish.kenvoy.xml" as="权限配置" />
</Page>
```

`src` 相对页面目录解析（如 `/leave/apply` + `./flow.kenvoy.xml` → `apps/leave/apply/flow.kenvoy.xml`），含 `..` 拒绝（`ImportInvalid`）；文件缺失报 `ImportNotFound`；根元素非 `<Flow>` / `<Publish>` 报 `UnknownImportType`。

**Table 节点（含数据源）**

```json
{ "type": "Table",
  "props": {
    "model": "请假单",
    "filter": { "申请人": "{{currentUser.id}}" },
    "sort": { "创建时间": "desc" },
    "pageSize": 20
  },
  "children": [
    { "type": "Column", "props": { "field": "请假类型", "label": "类型", "width": 100 } },
    { "type": "Column", "props": { "field": "审批状态", "label": "状态" } }
  ] }
```

## 4. Rust 类型与位置

建议放 `crates/kengine/src/ir/mod.rs`（serde + thiserror）：

```rust
pub const IR_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIr {
    pub version: String,
    pub project: ProjectMeta,
    pub models: BTreeMap<String, Model>,    // 有序：保证序列化输出确定（golden 测试/可复现）
    pub layouts: BTreeMap<String, Layout>,
    pub pages: Vec<Page>,
    #[serde(default)]
    pub flows: Vec<Flow>,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum Node {
    Row     { #[serde(default)] props: RowProps,     #[serde(default)] children: Vec<Node> },
    Col     { #[serde(default)] props: ColProps,     #[serde(default)] children: Vec<Node> },
    Section { #[serde(default)] props: SectionProps, #[serde(default)] children: Vec<Node> },
    Slot,
    Form    { #[serde(default)] props: FormProps,    #[serde(default)] children: Vec<Node> },
    Field   { #[serde(default)] props: FieldProps },
    Table   { #[serde(default)] props: TableProps,   #[serde(default)] children: Vec<Node> },
    Column  { #[serde(default)] props: ColumnProps },
    StatCard { #[serde(default)] props: StatCardProps },
    Text    { #[serde(default)] props: TextProps },
    /// 用户 DSL 自定义组件（kengine 解析成子树，或透传给渲染器）
    #[serde(rename = "custom")]
    Custom  { component: String,
              #[serde(default)] props: BTreeMap<String, Value>,
              #[serde(default)] children: Vec<Node> },
}
```

`#[serde(tag = "type")]` 内部标签，JSON 里就是 `"type": "Form"` 形态。因 serde 内部标签占用 `type` 字段，`Custom` 的组件名放在 `component` 字段，JSON 为 `{"type":"custom","component":"my-widget",...}`；内置组件与用户组件在渲染路径上统一为 `Node`。

## 5. layout 解析规则（kengine 实现）

```text
resolve_layout_chain(app_root, page_dir):
  chain = []
  从 page_dir 逐级向上到 app_root：
    若该目录存在 layout.kenvoy.xml → chain 头部插入该目录路径
  校验：app_root 的 layout 必须存在，否则报 LayoutNotFound
  返回 chain（外层在前）
```

同时校验：`Form.model` 引用的模型必须存在（`ModelNotFound`）、`Import` 的文件必须存在、字段类型合法。

## 6. 版本演进

- IR 顶层 `version` 字符串，kengine 产出、渲染器校验
- 新增组件类型 = 枚举加变体，**只加不改**，向后兼容
- 破坏性变更 → 升大版本，渲染器做 `version` 分派

## 7. 决策点

| # | 决策点 | 建议 | 状态 |
| :--- | :--- | :--- | :--- |
| D1 | 数据模型文件位置 | `apps/<应用名>/models.kenvoy.xml` | 已确认（2026-08-23） |
| D2 | IR→HTML 位置 | 独立 crate `kengine-html` | 已确认（2026-08-23） |
| D3 | dev 服务器框架 | axum + notify + tokio | 已确认（2026-08-23） |
| D4 | demo 热更新 | 整页刷新；SSE/WebSocket 后置 | 已确认（2026-08-23） |
| D5 | 解析库 | roxmltree | 已确认（2026-08-23） |
| D6 | Node 表示 | tagged enum + Custom 兜底（`component` 字段） | 已确认（2026-08-23） |
