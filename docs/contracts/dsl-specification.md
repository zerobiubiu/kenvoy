# 第一版已实现声明语法

状态：参考（现状文档）；基础语义在阶段 03 实现，自定义组件/函数和边界完善已通过阶段 08 核心验收。具体行为由 analyzer.rs/static_eval.rs、javascript/ 与测试定义。总体原因见 [DSL 设计](../architecture/dsl-design.md)，输出见 [Application IR](application-ir.md)。

## 入口、页面与资源

入口默认导出单个 app 或 page。支持 app、page、resource、text/textarea/number/boolean/date/datetime/select/file 八种字段、option、form、table、image、action、navigation、item、group、override；严格校验属性、父子关系、引用和冲突。[customer-app](../../examples/customer-app/src/app.tsx) 已由真实 CLI 创建并通过 check。

单段 page 路径可从直接字段推导资源；多段路径须明确 page.resource。显式资源优先，同资源页面字段结构必须一致；页面 labels 独立覆盖展示，不改变持久化模型。form 支持 resource、mode=create/edit、fields 字符串数组；table 支持 resource 和 columns。

dev 后续实现支持 React 常见函数模板入口，尚未发布，见 [0037](../changes/0037-i18n-reference-check-and-react-templates.md)：

```tsx
export default function App() {
  const title = '客户登记';
  return <app name="客户管理">
    <page path="/customers" title={title} />
  </app>;
}
```

支持命名/匿名函数、函数表达式、表达式/块箭头函数，以及静态别名和既有模块导入/重导出。函数不得有参数、async/generator；函数体仅允许顺序静态 `const`、空语句和一个直接 `return`，不执行组件。最终仍须是一个 `app/page` 根，不会把任意 HTML、嵌套函数里的 JSX 或运行时分支误作业务声明。

Hooks、事件、条件渲染和列表 `.map()` 放在普通 React UI 组件中；不能用它们动态生成 Schema。模板内静态常量可供返回树和客户端回调捕获，保持词法遮蔽及私有依赖边界；`.diu.tsx` 仍只展开既有静态声明，不改为执行函数。

## 表单权限与动作

公开 create form 只赋予单条表单策略对应的新增能力，不开放列表、详情、更新和删除；table 不自动授权读取。多个表单策略不得并集。create form 的 fields 必须覆盖资源中必填且无默认值的字段，否则在 check 阶段诊断，不能留下永远无法成功提交的表单。action 必须显式 name/handler/runtime=server|client|shared/permission=anonymous|user|admin，handler 需已导出的函数；Server/Shared 通过受限 QuickJS 和 Execute 权限调用，Client 用于浏览器函数登记。page 可声明 permission，导航继承目标页面角色。

## 字段约束

字段支持 name/label/required/default/index 及类型特定约束。text/textarea 支持 maxLength/unique；number 支持 integer/min/max/unique；select 的 option value 为唯一字符串。范围为空或默认值无效即报错。

## 静态结构与 Schema Component

静态求值支持顶层 const、跨模块引用/重导出、字面量、括号/TS 包装、明确支持的运算、数组/对象/spread、JSX/Fragment。函数调用/map、getter/method、可变绑定或无法证明安全的顶层副作用不能产生 Schema。求值有深度、节点、集合和步数限额。

.diu.tsx 默认导出静态声明，可跨文件组合并检测循环；当前不接受 Schema Props/children、解构求值或未导出的运行时函数，均明确诊断。普通 UI 保留模块引用，不展开其函数体；已绑定的小写组件也不能冒充内置字段。动态 UI Props 保留表达式/模块注册引用，函数对象不会 JSON 化。

## 客户端与服务端模块

server 模块及其运行依赖默认私有；.shared 或显式 shared 注册构成共享依赖边界。ServerFunction 根不能同时注册 Shared/Client，不能借共享标记把整个私有实现放入浏览器。客户端触达私有模块、动态 import、import.meta、未绑定 require 均拒绝；type-only 边不作为运行期依赖。普通运行引用保留原 import 门面，静态值和 Schema 仍追踪最终声明，避免转导丢失 CSS 副作用。

入口内的组件/函数和动态 Props 通过词法依赖切片进入客户端；未使用的 server import、默认 DSL 和无关常量不会进入切片。可捕获函数与静态 const，不能捕获可变声明、复杂解构或未知副作用初始化；超出范围时拆分独立 UI 模块。普通组件、Hooks/Props/CSS/第三方库、Schema Component、服务端 TS ESM 与 Promise 已在真实浏览器验收，细节见 [JavaScript Runtime](../architecture/javascript-runtime.md)。

## 组件重载与选项来源

app 支持 `<override name="select" component={CustomSelect} />`。可替换八种字段及 image/form/table/group/page 的显示层，字段结构和数据操作继续由 Runtime 管理。select 的 option 支持 `when={{本资源依赖字段: 值}}`；远程 select 使用 `optionsSource={{resource,value,label,filters,permission}}`，与静态 option 互斥。完整属性、权限、SDK 和示例见[组件与联动选项](component-bindings.md)。

## 文件字段与图片

`<file>` 支持多文件、`accept/maxSize/maxFiles`，值为严格的附件元数据数组；不支持 unique/index，不参与标量选项联动。`<image src="https://example.com/photo.png" alt="说明" width={320} height={180} fit="contain" />` 是展示节点，不生成业务列；alt 必须显式声明，地址及尺寸静态校验。完整用法、服务端授权、存储与取消规则见[多文件上传与图片展示](file-uploads.md)。

## 数据输入规则

数据长度按 Unicode 标量，必填空白用 Unicode White_Space；数字必须有限，integer 限 ±9007199254740991。date 为有效 YYYY-MM-DD，年份 0001..9999；datetime 要求 RFC3339 时区且最多 3 位小数，输出 UTC 毫秒，拒绝闰秒和越界。PATCH 缺失不等于 null；重复 JSON 键和未知字段拒绝。共享向量位于 crates/kenvoy/tests/fixtures/validation.json。
