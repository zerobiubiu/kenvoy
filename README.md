# Kenvoy

**汇批呈览 · 面向 AI 时代的语义化低代码平台**

> 写语义，出全端。

**Kenvoy** 是一套面向 AI 时代的语义化低代码平台。开发者（或 AI）使用专有的 **XML 业务语言**描述业务需求，Kenvoy 自动解析并生成完整的桌面端 / 移动端前端页面，同时关联数据模型、审批流程与权限控制。

它不是为“人”设计的拖拽式低代码，而是为 **AI 可书写、可修改**的确定性语义接口：

> 传统低代码靠拖拽生成坐标和配置 → 丢失业务语义、AI 无法操作。
> Kenvoy 让 AI 只输出极简的语义描述，由核心引擎负责解析、接线与生成，把复杂的实现细节留给平台。

## 命名来历

**Kenvoy** 读作 `/ˈkɛn.vɔɪ/`，由两个词根构成：

| 词根 | 来源 | 含义 |
| :--- | :--- | :--- |
| **Ken** | 古英语 | 知悉 / 看见 / 视野 |
| **voy** | 法语 / 英语（Voyage） | 旅程 / 流转 |

合在一起：**「知悉一切，流转万物」**。

中文名 **「汇批呈览」** 对应 Kenvoy 在 ToB 企业场景中的完整业务闭环：

| 字 | 含义 | 对应业务 |
| :--- | :--- | :--- |
| **汇** | 汇聚 / 归集 | 信息采集 |
| **批** | 批示 / 流转 | 流程审批 |
| **呈** | 呈报 / 呈文 | 数据上报 |
| **览** | 阅览 / 总览 | 数据分析 |

### 组件命名体系

| 层级 | 名字 | 简称 | 定位 | 开源 |
| :--- | :--- | :--- | :--- | :--- |
| 项目 / CLI | **Kenvoy** | `kv` | 语义化低代码平台 | ✅ 开源 |
| 解析器核心 | **Kenvoy-Engine** | `kengine` | DSL 解析引擎，输入语义输出布局与数据模型 | ✅ 开源 |
| 云平台 | **KenvoySpace** | `kspace` | 企业数字化云工作空间（控制台 / Dashboard） | 🔒 闭源 |
| 生产调度器 | **Kenvoy-Runtime** | `krt` | 生产环境调度中枢：路由分发、CI/CD、版本控制、日志、多服务协同 | 🔒 闭源 |

> **Kenvoy CLI 写 DSL → Engine 解析 → Space 承载 → Runtime 调度。**

## 仓库结构

当前为 Rust 工作区（Rust 1.98 / edition 2024 / resolver 3）：

```
kenvoy/
├── crates/
│   ├── kengine/   # 解析引擎库（lib，发布到 crates.io）
│   └── kenvoy/    # CLI（bin，依赖 kengine，发布到 crates.io）
├── AGENTS.md
├── Cargo.toml
└── LICENSE        # MIT
```

开源部分（`kenvoy` CLI + `kengine`）位于同一 Workspace，发布到 [crates.io](https://crates.io)；`KenvoySpace` 与 `Kenvoy-Runtime` 为闭源商业产品，不在本仓库。

## 核心设计

### XML 业务语言

业务描述使用 **XML** 作为载体（文件扩展名 `.kenvoy.xml`），结构化、AI 可自然生成与修改：

```xml
<!-- apps/请假管理/申请/page.kenvoy.xml -->
<Page>
  <Import src="./flow.kenvoy.xml" as="请假审批流" />
  <Title>填写请假单</Title>
  <Content>
    <Row>
      <Col span="8">
        <Section title="请假信息">
          <Form model="请假单">
            <Field name="申请人" readonly default="{{ current_user.name }}" />
            <Field name="请假类型" type="select" options="年假,病假,事假,调休" />
            <Field name="请假天数" type="number" suffix="天" />
            <Field name="事由" type="textarea" rows="4" />
            <Validation>
              <Rule field="请假天数" expr="> 0">请假天数必须大于 0</Rule>
            </Validation>
            <Actions>
              <Submit action="create" flow="请假审批流">提交申请</Submit>
            </Actions>
          </Form>
        </Section>
      </Col>
    </Row>
  </Content>
</Page>
```

### 文件系统即路由（参照 Next.js App Router）

- 目录结构直接映射为路由与布局层级
- **layout**：定义页面公共结构（导航、侧边栏、页脚等）
  - 逐级继承：同级 > 父级 > 父父级 > … > 根 layout
  - 子目录可放置自己的 `layout.kenvoy.xml` 覆盖父级
  - `apps/` 根下必须存在根 `layout.kenvoy.xml`
- **page**：页面内容，一个目录一个 `page.kenvoy.xml`，自动关联最近匹配的 layout
- **flow**：审批流 / 工作流，只出现在 page 同级，必须被 page 显式引入
- **publish**：权限 / 发布，只出现在 page 同级，必须被 page 显式引入

### 目标项目结构

```
项目根目录/
├── apps/                    # 所有应用（路由即路径）
│   ├── layout.kenvoy.xml    # 根布局（必须存在）
│   ├── dashboard/           # 工作台仪表盘
│   └── 应用名/
│       ├── layout.kenvoy.xml
│       ├── module/          # 应用私有组件
│       └── 页面/
│           ├── page.kenvoy.xml
│           ├── flow.kenvoy.xml
│           ├── publish.kenvoy.xml
│           └── layout.kenvoy.xml   # 可选，覆盖父级
├── module/                  # 全局通用组件（表单、表格、图表等）
├── i18n/                    # 国际化（zh / en）
├── public/                  # 静态资源（/ 路径直出，不可在运行时修改）
├── dist/                    # 构建产物（自动生成）
├── docs/                    # 项目文档
├── packages/                # 外部依赖库
├── kenvoy.toml              # 项目配置（类似 Cargo.toml）
├── .kenvoy/                 # 本地元数据 / 缓存（自动生成）
├── .gitignore
└── .env                     # 环境变量
```

### 设计原则

| 原则 | 说明 |
| :--- | :--- |
| 语义优先 | 只描述「做什么」，布局、交互、数据联动由引擎自动推导 |
| AI 友好 | 纯文本结构化，AI 可自然产出与修改，无需关注底层实现 |
| 全端自动生成 | 一次描述，同时生成桌面端与移动端页面，自动关联数据模型 |
| 约定大于配置 | 目录结构即路由，减少配置负担 |
| 关注点分离 | page / flow / publish 各司其职，layout 逐级复用 |

## 快速开始

运行示例项目（请假管理 demo）：

```bash
cd examples/leave-app
cargo run -p kenvoy -- dev          # 启动开发服务器，浏览器打开 http://127.0.0.1:5173
```

修改 `apps/` 下的 `.kenvoy.xml` 会自动重新构建并刷新页面。

开发命令：

```bash
cargo build --workspace
cargo run -p kenvoy -- --help     # 查看命令
cargo run -p kenvoy -- hello      # 打印问候语
cargo run -p kenvoy -- info       # 显示引擎信息
cargo run -p kenvoy -- init my-app --title 我的应用   # 初始化新项目骨架
cargo run -p kenvoy -- build                          # 构建静态产物到 dist/
cargo run -p kenvoy -- dev --port 8080                # 自定义端口
cargo test --workspace            # 运行测试
```

安装方式（发布到 crates.io 后可用）：

```bash
cargo install kenvoy
```

## 状态与路线图

### 已完成

- [x] Workspace 骨架（kengine 库 + kenvoy CLI + kengine-html）
- [x] `kengine`：XML 解析（`<Layout>` / `<Page>` / `<Models>` / `<Flow>` / `<Publish>` + `<Import>` 引入 + `<Validation>` 校验规则）
- [x] `kenvoy` CLI：`kv dev` / `kv init` / `kv build`；`kenvoy.toml` 配置
- [x] `kengine-html`：IR → HTML 渲染（SSR 核心，HTML 转义防 XSS）
- [x] 表达式引擎 `kengine::expr`（中文标识符、`{{ }}` 绑定、`empty()` / `非空`）
- [x] 表单校验执行 `kengine::validation`
- [x] 审批流状态机 `kengine::workflow`（推进/退回/条件/更新/通知）
- [x] demo 客户端交互：表单提交 + 表格数据（dev 数据 API）
- [x] PostgreSQL 18+ 持久化（`kengine-db`：模型动态建表 + CRUD、用户/角色/部门/通讯录/会话/文件元数据表；dev 数据 API 接入 PG，表单校验接入提交）
- [x] 用户与权限系统基础（`/__kenvoy/auth/*` HTTP API + `kv user/role/contact/department` CLI：注册/登录/登出/会话、账号等级、角色分配、部门树、通讯录含无账号人员与账号关联）
- [x] Tabler 组件基础（Bootstrap 5 风格类名：`row` / `col-md-*` / `card` / `form-control` / `form-select` / `table`，移动端响应式栅格）
- [x] 静态构建增强（`kv build` 输出 `assets/tabler.min.css` / `__kenvoy/runtime.js` / `__kenvoy/ir.json`，页面注入运行时）
- [x] 数据访问鉴权（数据 API 登录保护；`DataFilter` 行级过滤按角色；`publish` 字段写权限；模型表列迁移）

### 规划中

- [ ] S3 对象存储抽象与文件上传（`public/` 不可运行时写入；`storage_object` 元数据表已就绪）
- [ ] 业务组件扩展（日期范围、上传、图表、流程查看器；DSL 级自定义组件 `<Component>` / `module/`）
- [ ] 用户与权限系统（`auth/` 原生：users / roles / departments / sessions；页面级 + 字段级鉴权；`DataFilter` 行级过滤）
- [ ] 组件库扩展（日期范围、上传、图表、流程查看器；参考 [Tabler](https://github.com/tabler/tabler)）；DSL 级自定义组件（`<Component>` 内联 + `module/`）
- [ ] 移动端自适应（`kv-col` 断点）
- [ ] 静态构建增强（`kv build` 注入运行时 + 数据后端 / 声明静态站模式）
- [ ] `kv deploy`（需闭源 KenvoySpace）
- [ ] 桌面端 / 移动端自适应渲染
- [ ] 数据模型自动关联
- [ ] KenvoySpace 云平台（闭源）
- [ ] Kenvoy-Runtime 生产调度器（闭源）
- [ ] AI 辅助 DSL 生成

## License

MIT

`kenvoy` CLI 与 `kengine` 库采用 MIT 许可证；`KenvoySpace` 与 `Kenvoy-Runtime` 为闭源商业产品。
