# 声明解释与运行时分工

状态：现行；静态 DSL、Resource、API、页面、后台和 Action 解释均已实现。

## 静态业务分析

入口默认导出必须能静态定位到 JSX 根。分析器跟随常量、模块导入导出、静态属性展开、Fragment 与 .diu.tsx 组合；不会执行用户函数、map 或 getter 来猜测 Schema。普通 UI 函数体的 JSX 保持为组件引用，不展开成字段。

第一遍收集资源与页面草案，第二遍绑定引用、合并允许的简写声明并核验路由、字段和权限。显式资源优先，展示标签不改变持久化约束；同名但不同 Schema 明确报错。实际标签和参数规则见 [DSL 规范](dsl-specification.md)。

客户端与服务器模块边界在构建前检查，动态加载和未经建模的 require/import.meta 不能绕过图验证。无法静态求值的 UI 属性使用 ModuleReference，Schema 属性则必须给出诊断。业务分析器不把全部表达式强行转成 JSON。

## 运行时解释

| 模型 | 当前执行方式 | 数据/权限边界 |
|---|---|---|
| Resource | 统一字段校验及 Database CRUD | 仅已登记资源，SQL 参数绑定 |
| Permission | 服务端按一次操作选择完整策略 | role、operation、form、fields 同时匹配 |
| API | Axum 通用资源路由 | 会话、Origin、CSRF、代际、请求大小和参数查重 |
| Page/Component | 受控 DTO 驱动 React 注册表 | 无宿主路径或函数对象，数据仍通过 API |
| Admin | 与普通页面共享资源和 API | 管理员授权，列表、详情、搜索、筛选、排序、分页与 CRUD |
| Action | TS 去类型、白名单 ESM、独立 QuickJS 上下文、JSON/Promise | Execute 权限、CSRF、代际、时间/内存/队列限额；无文件或网络宿主 API |

页面与资源不会生成专用 Controller、Repository 或 React 业务源文件。通用组件树由 Runtime 解释，普通 React 模块仍需真实构建。

## 错误与更新

语言诊断、业务诊断、迁移失败和 HTTP 错误各自保留类型边界。HTTP 只返回可公开的错误码、字段问题和关联编号。语言/业务错误发生时不触及 SQLite；迁移失败保留旧模型和数据。开发协调器先准备完整候选，再调用代际发布接口；语言 session 成功不代表完整应用发布成功。
