# DDD 分层、目录与模块规模规范

状态：参考；治理：现状规范。本篇定义已落地的依赖方向与拆分规则，决策见 [ADR 0012](../decisions/0012-ddd-layers-and-module-boundaries.md)，实施及实际验证范围见 [0016](../changes/0016-ddd-structure-and-module-split.md)，不由目录存在推定验收成功。返回[架构索引](README.md)。

## 领域与边界

| 边界 | 负责 | 不负责 |
|---|---|---|
| KEngine 语言域 | 通用源码模型、语义绑定、模块图与语言分析 | 页面、资源、HTTP、数据库、认证或 React 业务 |
| Kenvoy 宿主域 | 资源/字段定义、声明快照、验证和授权规则 | 硬编码客户/订单等用户业务，或把 SQL/HTTP 当领域模型 |
| 用户应用 | 通过 DSL 定义具体资源、表单和扩展组件 | 修改框架的领域或复制另一份服务端 Schema |
| 工程与测试 | 真实 CLI/进程夹具、构建编排、结构和文档门禁 | 进入产品依赖闭包或承载产品业务 |

`ApplicationIR` 是已验证、不可变发布的声明快照；资源定义包含字段约束，字段/角色/权限及附件元数据沿用同一模型。领域规则是纯函数或已有模型行为，不强行增加 ORM 实体基类、事件总线和空适配器。

## Kenvoy 目录

```text
crates/kenvoy/src/
├── main.rs                 # 参数解析、日志、执行与报告的薄入口
├── domain/                 # 模型、来源位置、纯校验/选项/授权/凭据规则
├── application/            # 编译、身份、资源、执行与发布用例/真实端口
├── infrastructure/         # SQLite、SWC/QuickJS、文件项目、资产与客户端构建
├── interfaces/             # CLI 与 HTTP 解析/响应适配
└── bootstrap/              # 具体适配器装配、进程与开发生命周期
```

- domain 不得依赖 application/infrastructure/interfaces/bootstrap，也不得执行文件、网络、数据库、UI、系统随机或密码驱动操作。`PathBuf/SourceSpan` 作为值不是 I/O。
- application 不得引用 infrastructure/interfaces/bootstrap；用例通过实际端口工作。传输状态码、HeaderMap、Connection、SWC AST 或 QuickJS Context 不进入应用契约。
- infrastructure 实现应用端口，不反引接口或 bootstrap。SQL 事务和驱动错误在此维护；必要失败分类由端口提供，HTTP 不依赖具体数据库错误变体。
- interfaces 解析请求、执行传输防护并映射用例结果，不复制领域规则。HTTP 具体泛型绑定集中在外层 wiring，实际仓储不能泄露为控制器的业务操作入口。
- bootstrap 是唯一允许整体装配的根。项目文件发现/产物/Node 工具链等真实适配器和进程资源在这里连接，不把全局聚合错误反灌基础设施。

查询、附件与身份存储 DTO 属于其真实用例/端口；SQLite 内部物理 Schema/Diff 属于适配器。不能仅把驱动类型改名再放到 domain。

## KEngine 目录

```text
crates/kengine/src/
├── lib.rs                  # 原有公开根 API 门面
├── domain/                 # 自有 IR、位置/诊断、语义/模块/链接纯规则
├── application/            # 共用分析、增量缓存与静态 Host 端口
├── infrastructure/         # SWC 和文件系统
└── interfaces/             # Engine/AnalysisSession 的真实装配入口
```

公开根类型、字段、序列化与方法契约保持；内部平铺实现路径不保留兼容层。Parser 与 resolver 的调用顺序、精确 external、候选顺序、UTF-16/raw/span 和最后有效结果不因目录重构改变。

## 前端目录

```text
crates/kenvoy/frontend/src/
├── main.tsx                # 实际页面入口
├── sdk.ts                  # @kenvoy/client 实际公共门面
├── app/                    # 路由、会话、应用装配
├── domain/                 # 类型、文件/日期/字段/选项纯规则
├── application/            # 客户端端口、错误与上传生命周期
├── infrastructure/         # 同源 Fetch 与协议适配
└── presentation/           # Context/Hooks、字段/表单、表格、页面树、后台和 UI
```

Context/SDK 不得回引虚拟组件注册表的页面树；注册组件与递归 RenderNode 保持同一职责模块，避免新环。应用上传控制器不依赖具体 Fetch 实现。Node 原生规则测试保持可执行的 `.ts` 导入链，不用 SDK 门面把 React/TSX 意外带进纯规则测试。

新增或迁移前端模块必须同步 Rust 内嵌 SOURCES、框架源码缓存、Vite 入口/白名单、开发镜像与真实 HMR URL；保持安全镜像，不能退回直接公开用户源文件。外部 SDK 模块标识仍为 `@kenvoy/client`。

## 规模与拆分

1. 自有 Rust/TS/TSX/MJS/CSS 代码每文件最多 500 物理行，目标 400 行内；测试同样受限，避免生产文件靠内嵌测试膨胀。
2. 达到目标区间时先识别模型、策略、流程、I/O、展示或夹具职责；超过硬上限前完成拆分。单函数没有独立职责时不强拆，不建立空 service/repository 目录。
3. 文件只依赖真实需要的项；生产代码不使用 `use super::*` 或 `crate::*` 兜底。兄弟模块按调用关系最小化可见性，不把全部字段统一开放到 crate。
4. 领域规则抽取保留差异化语义，不能把 session token、上传 ID、资源名或路径的不同规则混成万能字符串工具。
5. 公共门面与内部兼容 shim 不同：KEngine 根 API、SDK 和测试库公共入口可以保持；已经迁移的内部旧根路径必须删除，调用方直接使用规范位置。
6. 数据 JSON、样例二进制、锁文件、依赖和生成产物不按代码行数处理；不得通过 include 字符串、巨型 shared 文件或忽略名单绕限制。

## 工程门禁与验证

`cargo xtask architecture` 与 `cargo xtask check` 包含源码目录、规模和层依赖约束；工程工具按 checks/commands，测试夹具按 process/fixture/protocol 组织，不伪造业务 domain。

重构验证包括公开 API 编译、KEngine 独立检查/测试、前端原生测试与构建、Workspace 回归、真实 CLI/浏览器及旧库读回。四例 `inspect` 与 CLI `--help` 留基线逐字比较；历史日志、测试数量或 0 tests 不作为本轮执行证明。具体参数/过滤与执行边界由当次记录说明。
