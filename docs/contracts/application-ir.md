# Application IR 与客户端描述协议

状态：参考（现状文档）。本文维护 Application IR v1 与客户端描述契约；实际类型位于 `crates/kenvoy/src/application.rs`。同代发布、文件监听与 build/run 已接入，覆盖范围见[生命周期验收](../verification/0011-cli-lifecycle.md)。

## 单一业务模型

新增兼容字段：ApplicationIR.component_overrides 保存系统组件名称到模块引用 ID 的映射，缺省为空；FieldDefinition.options_source 保存受限跨资源选项配置，缺省为空；SelectOption.when 保存静态依赖条件，缺省为空。值仍遵守原字段类型，不因控件重载改变。客户端投影携带渲染所需定义，选项查询仍需独立授权；完整规则见[组件与选项契约](component-bindings.md)。

Application IR 属于 Kenvoy。输入来自 KEngine 的 ProgramIR、模块绑定和表达式结构；分析器只沿入口默认导出的表达式及显式 Schema Component 访问声明。不能将模块中所有 JSX 搜索结果当作应用根。静态求值只存在于 Kenvoy，不改变 KEngine 对标签、标识符和表达式的通用含义。

同一份经过验证的模型驱动 Schema Diff、数据库执行计划、HTTP 权限、页面描述和 Admin。模型构建失败时不产生可激活的半成品。运行时持有不可变版本化快照，请求不重新读取源码。

## 对象及标识

| 对象 | 必需内容 | 约束 |
|---|---|---|
| Application | format_version、name、resources、pages、actions、navigation、permissions、module_references | 拒绝未知协议版本；使用确定顺序序列化 |
| Resource | name、fields、访问策略 | 名称经过白名单验证；不允许内部表名 |
| Field | name、kind、label、required、default、约束、source | name 与显示 label 分离；默认值在分析时验证 |
| Page | path、title、components、访问策略 | 正常化静态路径；不占用保留前缀 |
| Component | type、props、children、资源或模块引用 | 动态值用注册引用；不把函数对象放进 JSON |
| Action | name、runtime、permission、handler 注册 ID、source | 服务端执行必须明确；不得隐式公开 |
| Permission | resource/action、operation、role、可提交字段集合 | 服务端执行；缺失策略即拒绝 |
| ModuleReference | 模块标识、导出名、用途、源码位置 | 实际路径只在宿主产物出现；客户端用注册 ID |

对象保留来源 path/span，用于重复声明、无效引用和展开链的诊断。应用业务标识跨重构保持稳定；KEngine 的 NodeId/ExpressionId/SymbolId 是单次分析的定位标识，不能直接当作持久化资源身份。

## 静态声明求值

支持范围由实现及测试明确列出。基础闭环包括字面量、括号和 TypeScript 类型包装、const 绑定、有序对象/数组和静态 spread、静态 JSX 与 Fragment、Schema Component 的跨模块默认导出。变量可解析不表示值可静态求解；let/var、函数调用、运行时 map、闭包和未知表达式不能用于推导字段。

求值器有递归深度、展开节点和集合大小上限，并检测常量/Schema 循环。所有需要静态值的位置在失败时产生阻断诊断，不能退回空字符串、空字段列表或忽略子节点。普通 UI 组件转为模块引用，不展开其函数体。

## 客户端投影

客户端投影由 HTTP 显式构造，使用独立格式版本，只包含当前身份可见页面、所需字段约束、导航、动作公开标识以及组件注册表所需信息。它不携带源码、绝对路径、密码哈希、Session、数据库路径、服务端函数实现或环境变量。匿名可见页面所需字段的展示合并不改变单条表单权限规则。

前端通过统一 Runtime 解释页面组件树，通过同一 Field 定义实现创建与编辑表单。Admin 使用相同 Resource 描述，不创建第二套字段模型。服务器仍逐请求验证权限和字段；客户端的允许操作列表只用于展示。

默认公开 create form 只授予对应资源的受控新增能力。声明 table 不授予公开列表；声明资源也不授予匿名读取、更新或删除。Schema 中存在某字段不代表任何用户可任意写入该字段。

## 激活与产物

构建产物保存宿主 `application.ir`、`client/` 资产和 `modules/server.json`，manifest 校验格式、框架版本和内容摘要。客户端投影由 HTTP 在运行时按身份构造，没有独立的静态投影文件。新增资源及安全增列通过迁移后，再发布同代业务模型；不允许数据库用新结构而请求仍按旧字段策略写入。

ComponentDefinition 使用 type 字段区分 form/table/group/custom/text。Form 包含资源、字段名投影及页面 labels；PropValue 使用 kind=literal/module 区分 JSON 值和注册 ID。ModuleReference 保存模块导出或表达式来源，属于宿主数据。SourceLocation 仅用于宿主诊断。未知 Application IR 版本反序列化失败；Field.default 区分未声明与显式 null。

ModuleReference.kind 明确区分 ClientComponent、ClientFunction、ServerFunction、SharedFunction 和 ClientExpression。普通运行引用保留原模块门面与导出名；表达式保存原始 span，客户端经词法依赖切片生成工厂。组件/函数/表达式注册表与当前应用代际一起构建，不把运行期函数对象放入 JSON，也不承诺 module_1 等注册 ID 在不同分析代际不变。

服务端 PreparedBundle v1 与 Application IR 分开版本化。HTTP 初始化及发布验证所有 Server/Shared handler 均已编译登记；请求持同代读锁，发布将模型、静态目录、函数包和 generation 同时替换。开发协调器准备完整候选后调用该接口；取消安全与失败恢复已有测试。

相关语义见 [DSL 设计](../architecture/dsl-design.md)，运行与切换见 [Runtime 设计](../architecture/runtime-design.md)、[JavaScript Runtime](../architecture/javascript-runtime.md)，持久化见 [数据库设计](../architecture/database-design.md)，实际验证见 [阶段 03](../verification/0005-dsl-cli.md)及[阶段 08](../verification/0010-typescript-extensions.md)。
