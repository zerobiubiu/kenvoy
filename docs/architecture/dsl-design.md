# DSL 与 Application IR 设计

状态：参考（现状文档）。本篇解释静态声明、资源命名与权限的设计原则；实际标签/属性见 [语法规范](../contracts/dsl-specification.md)，业务模型见 [Application IR](../contracts/application-ir.md)，执行与验收见 [Runtime](runtime-design.md) 和[最终验收](../verification/0012-final-acceptance.md)。

## 解释原则

只解释项目入口默认导出的静态声明树及明确引用的 Schema Component。模块中任意函数体出现的 JSX 不自动成为应用声明。支持 app 根，也允许单 page 根并由 Kenvoy 补应用默认值；其他根结构必须有明确诊断。

静态值允许字面量、可确定的常量、只读对象/数组及已实现的确定性运算。常量跨模块定位需符号图；递归、循环、函数调用、未知 spread、动态数组映射一律保留在 Generic IR，并在需要静态 Schema 的位置报告阻断诊断。第一版不执行任意用户函数生成数据库结构。

## 标签注册表

每项实现时必须有属性白名单、类型、必需项、父子约束、位置和转换规则；未知属性、字段标签或非法父子关系均报错。

| 标签 | 关键属性 | 允许位置/子节点 | 业务语义 |
|---|---|---|---|
| app | name | 根；resource、page、action、navigation、override | 应用容器；当前不支持标题或主题属性 |
| override | name、component | app；无子节点 | 将系统显示组件关联到普通 React 引用；不改变字段模型 |
| resource | name | app；字段、Schema Component | 持久化模型 |
| page | path、title、可选 resource | app 或根；form、table、字段、UI、group | 可访问页面与组件树 |
| text/textarea | name、label、required、maxLength、default、unique、index | resource 或页面字段组 | 字符串字段 |
| number | name、label、required、integer、min、max、default、unique、index | 同上 | 整数或有限实数 |
| boolean | name、label、required、default、index | 同上 | 布尔字段 |
| date/datetime | name、label、required、default、index | 同上 | 日期或带时区时间值 |
| select | name、label、required、default、index、optionsSource | 同上；静态 option 或远程来源 | 字符串选项，提交按来源和依赖重验 |
| option | value、label、when | select | 静态枚举与依赖条件，value 唯一 |
| form | resource、mode | page/group | create/edit 表单；缺省模式 create |
| table | resource、可选列配置 | page/group | 资源列表；不因出现表格开放匿名读 |
| action | name、handler、runtime、permission | app | 有明确执行位置和策略的操作 |
| navigation | 可选 label | app；item、group | 导航描述 |
| item | label、path | navigation/group | 目标必须有效且不越权 |
| group | 可选 label | page/navigation；对应上下文子节点 | 布局/导航分组，不创建资源 |

`page.resource` 和 action 的 runtime/permission 均已实现，准确要求见[语法规范](../contracts/dsl-specification.md)。显式资源绑定避免多段页面路径产生意外表名；显式 Action 环境与权限避免含糊的服务端暴露。字段与 form/table 不能明确绑定资源时报告诊断。

## 资源命名与简写

资源和字段标识首版使用 ASCII 小写标识：首字符 a-z，后续 a-z、数字或下划线；拒绝系统保留名、id、created_at、updated_at 以及内部前缀。name 用作稳定业务标识，label 用于任意中文显示。表名由经过验证的 Resource IR 派生，HTTP 输入不能指定任意数据库表。

单段页面路径 `/customers` 的直接字段可推导资源 customers，并产生 create form。路径先统一尾斜杠、重复斜杠和大小写策略：路径大小写保留且区分；只接受明确的正常化形式，拒绝百分号编码分隔符、反斜杠、点段和保留前缀。隐式资源名须符合上述标识规则。

多段页面直接字段不凭末段猜表名；须用 page.resource 明确绑定，或使用显式 Resource + form。`/customers/new` 中的 form resource=customers 与 `/customers` 共用一份资源，不生成 customers_new 表。通过两遍分析先收集全部显式/隐式资源，再解析页面引用，允许页面引用在其他页面推导的资源。

同名显式资源优先：页面字段必须与它的字段名称、类型和约束一致，显示 label 可由页面覆盖，冲突报错。多个页面绑定同一隐式资源时也要求结构兼容，不悄悄取并集、不依页面顺序覆盖；不同字段集合要改用显式资源与明确表单投影。重复显式 Resource 或重复字段始终报错。

## 字段语义

字符串长度前后端统一按 Unicode 字符计数；required 字符串拒绝空串和全空白；不擅自清洗电话等业务值。number 必须为 JSON 数字、有限值，min/max 为包含边界。integer 首版限定为浏览器安全整数范围 `−(2^53−1)` 至 `2^53−1`，与 min/max 取交集；服务端检查后转换为 i64 绑定 SQLite，禁止舍入或截断。可选空输入按字段约定变成 null；PATCH 中缺失表示不修改，显式 null 仅可用于可空字段。未知字段、重复键和类型不符拒绝。

date 使用有效日历日期；datetime 输入须有时区，统一保存为约定精度的 UTC 字符串，不接受无时区歧义。boolean 只接受布尔值。select 首版选项 value 为字符串，值必须属于枚举。默认值为可静态验证的类型匹配值，同时在服务端与允许的 SQL 默认约束中执行。

字段支持 required、default、unique、index 和对应类型边界。unique 属于业务约束且生成索引；普通 index 仅优化。首版不开放任意 SQL default、CHECK 文本、任意 FK 目标或关系字段 DSL；内部 Schema Engine 仍能表达主键、外键和受控约束，以支持认证表及未来关系能力。

前后端由同一字段规则投影驱动，数据库约束再兜底。只靠 HTML required/min/max 不算前后端验证一致；关键边界需通过同一测试向量在浏览器与服务端分别验证。

## 组件与函数分类

| 类别 | 定位与处理 | Schema | 客户端 |
|---|---|---|---|
| Schema Component | `.diu.tsx` 默认导出静态 JSX；递归静态展开 | 是 | 不直接执行 |
| UI Component | 普通 TSX 导出组件；解析为模块+export 注册引用 | 否 | React 渲染，允许 Hooks/Props/CSS |
| Server Function | `.server.ts` 或显式 server 注册入口 | 否 | 禁止进入客户端可达图 |
| Shared Function | 明确共享入口、纯 JSON 计算 | 否 | 可进入两端，依赖也须合规 |

后缀不是唯一安全边界。Kenvoy 结合符号、导出和可达图检查直接/间接导入，不能通过换文件名或重导出绕过服务端隔离。type-only import 单列，转译后须不存在运行依赖。未知动态 import 无法证明安全时阻断客户端构建。

Schema Component 展开支持静态组合和确定常量，循环展开、未知动态字段、重复字段给出源文件及引用链。普通 UI 内部 JSX 不扫描成数据库字段。动态 Props、事件和自定义组件保留注册 ID 与表达式/模块引用，不能在页面 JSON 中放函数或使用 eval。

Action handler 通过符号图定位；必须明确 runtime 和 permission。首版服务端 action 仅可信项目纯函数，JSON 输入输出，可返回 Promise；API 根据 Action IR 分发，而非将服务端实现发给浏览器。

## Application IR v1

业务模型把资源、页面、动作、权限及模块引用统一交给运行时解释。源码位置用于诊断；资源名等业务身份与单次分析节点 ID 分开。模块注册 ID 随分析代际变化，不应当作持久化业务标识。

精确字段和客户端投影统一维护在 [Application IR](../contracts/application-ir.md)。构建版本与文件哈希由产物 manifest 管理；HTTP 根据身份生成浏览器所需描述，不直接公开内部 IR、服务端源码或私有模块路径。

## 默认权限

公开页面中的显式 create form 是受控公开提交声明，与页面简写等价，保证给定 customer-app 不必增加隐藏的 public 属性即可登记。它只授予该资源、该表单字段范围的 create 能力；服务端根据生成的 Permission IR 校验，不信任客户端声称来自哪个页面。

仅声明 resource 不开放匿名 API。列表、详情、修改、删除默认拒绝；管理员经认证和 CSRF 校验后可管理。普通已登录用户不自动成为管理员，只有明确策略才能访问相应操作。公开 create 返回该次新建记录的公开投影，不赋予后续读取能力。table 不隐含公开列表授权。

form 限制字段投影和 Resource 约束共同生效；多个公开表单不能通过拼接字段绕过各自权限。每次请求只匹配一个合法提交策略。认证、限流、同源和 CSRF 规则见 [运行时设计](runtime-design.md)。

## 路由和诊断

保留 `/api`、`/admin`、`/_kenvoy` 命名空间；用户页面不能抢占。统一路由注册表在启动前检查页面重复、规范化冲突、动态/静态匹配歧义和资源/动作名称冲突。首版静态路径优先；未实现的动态路径语法直接诊断，不交给 SPA fallback 静默处理。

语义错误包含独立 KV 诊断码、类别、文件、行列、原因、关联声明和建议。重复字段、非法嵌套、未知属性、无效 Resource、路由冲突、动态 Schema、客户端触达服务端分别有测试。代码编号随实现登记，当前未虚构已存在诊断编号。

相关决策：[ADR 0004](../decisions/0004-kenvoy-application-interpreter.md)；阶段状态与后续方向见 [实施路线](../development/development-roadmap.md)。
