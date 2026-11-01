# ADR 0010：原生默认组件、组件重载与统一数据绑定

状态：现行。前置：[ADR 0006](0006-react-csr-runtime.md)。替代其默认 MUI 选择，保留 React CSR、通用页面解释及构建适配方向。

## 背景与问题

用户要求移除框架 MUI 依赖，允许应用选择 React 组件库，并重载系统组件。select 除静态枚举外，还需按其他字段联动并查询其他资源。组件替换不能改变字段的后端类型、校验、权限或持久化语义。

## 可选方案

1. 保留 MUI 包装器：迁移较少，但仍限制框架体积与样式依赖，不符合用户要求。
2. 每个组件自行请求及定义 Schema：自由度高，但会产生多份字段规则、联动竞态与权限差异。
3. 默认原生 React 控件，运行时持有数据绑定，应用只替换呈现：选择此方案。

## 决策

默认组件使用 React、TypeScript、原生 CSS 和语义 HTML。框架构建器只统一 React、React DOM、React Router；其他组件库从应用显式声明且锁定的 npm 依赖解析，沿用客户端私有模块检查。无需为库建立专用白名单，也不承诺不兼容当前 React 或依赖服务端能力的包可以在浏览器运行。

`override` 声明把系统组件类型关联到普通 React 模块引用。Runtime 保有数据和操作状态，传递只读字段/资源模型、规范值、变更回调与状态；SDK 为普通自定义组件提供同一资源与 Action 通道。渲染层不能更改 Application IR 或绕过服务端校验。

select 始终存储 `string | null`。静态选项可带依赖条件；远程选项声明来源资源、值列、标签列、依赖筛选和最小角色。专用接口仅返回选项投影，不授予来源资源的列表或详情权限。创建与合并后的 PATCH 在数据库写事务内重新验证值和依赖。

## 原因与已知代价

受控 React 控件要求 value 与 onChange 同步，见 [React select](https://react.dev/reference/react-dom/components/select)。统一 React 解析避免重复实例，见 [Vite dedupe](https://vite.dev/config/shared-options#resolve-dedupe)。原生控件减去 UI 库依赖，但浏览器外观略有差异，焦点、弹窗与错误反馈需自行实现和验收。

远程选项不是通用关系字段：来源记录改变后，既有记录不会被自动级联更新；下一次编辑提交会重新验证。无法访问的源资源不会因自定义控件而开放，具体投影规则见[组件与选项契约](../contracts/component-bindings.md)。

## 架构影响

所有新增语义只位于 Kenvoy，KEngine 继续仅解析普通 JSX、表达式和模块。Application IR 的可选新增字段默认空，旧 v1 描述仍可读取；未知或无效声明明确报错。源码变更后的产物按既有指纹重建，不迁移用户数据以完成 UI 替换。
