# Generic IR v1

状态：参考（现状文档），含阶段 02 表达式、语义和模块链接；结构与序列化协议见 [ir.rs](../../crates/kengine/src/ir.rs)。

## 格式与源码

GENERIC_IR_VERSION=1，ModuleIR 和 ProgramIR 各带 format_version，反序列化时拒绝未知版本（含嵌套模块）。它独立于 SWC 版本、Cargo package 版本及 Kenvoy Application IR。模块保存一份完整原始源码，不在每层嵌套节点复制 raw；SourceSpan.raw 或 ModuleIR.raw 用半开字节区间恢复精确片段。

SourceSpan 同时保存 start/end 字节和 start_position/end_position。Position 从 1 开始，以 Unicode 标量计列；处理 CRLF、CR、LF、Unicode 行分隔符和 BOM，原始字节切片保持不变。NodeId 与 ExpressionId 仅是所属模块 arena 的索引，不承诺跨任意文件修改稳定。

ModuleIR 包含 path、source、imports、exports、declarations、nodes、expressions、jsx_roots 和 semantic。ProgramIR 包含模块列表、静态依赖图、module_bindings 和诊断。module.node、module.expression、module.raw 对无效索引/范围返回 None；调用方不得用另一个模块的 ID 访问当前模块。

## JSX

NodeKind 表示 Element、Fragment、Text、Expression child 或 Spread child。ElementIR 保存名称、属性顺序、子节点 ID、self_closing 及 opening/closing span。名称区分 Identifier、Member 和 Namespaced，各部分有位置；text() 只提供名称展示，不增加业务含义。

AttributeIR 含独立 span；AttributeKind 为 Named 或 Spread。Named.value=None 表示裸布尔属性，保留它与显式 true 表达式的语法区别。属性值区分字符串、表达式容器、Element 和 Fragment；空表达式显式为 None，不能等同 unknown 或 null。

Visitor 遍历完整模块，可发现函数、类方法、箭头函数、属性/子表达式中的 JSX；发现不等于执行函数或静态展开 Schema。直接 JSX 父子和表达式内 JSX 引用分别保留，同一出现位置由 arena 去重。

## 字面量与动态表达式

StringLiteralIR 使用 value_utf16 精确保留 JavaScript 字符串的 UTF-16 code units，包括孤立 surrogate；to_string 只有无损转为 Rust UTF-8 时成功。JSX 文本也保留 UTF-16，不使用有损字符串作为唯一表示。原始引号、转义和语法始终可从 span 恢复。

LiteralIR 区分 String、Number、Boolean、Null、BigInt、Regex。非有限数值不直接序列化为非法 JSON 数字；原始数字形式仍在 source/span 中，不能将 None 误认成 JS null。BigInt 和正则不强行退化为普通 JSON 属性值。

ExpressionIR 保存 kind、span、可选字面量、direct_identifier、shape 和内部 JSX 引用。shape 提供直接 JSX 节点、括号/TS 包装、标识符、字面量、有序对象/数组、运算符、条件、成员和模板的子表达式 ID。数组 holes 与 spread、对象属性与 spread 顺序、getter/setter/method 均保留。调用、函数及其他未建模结构保留 Opaque、kind 和精确 source/span；不能把 Opaque 当作无副作用或空值。direct_identifier 仍是文本，绑定需要 semantic。

## 模块与声明

ImportIR 保留 source、整个声明位置、attributes、type-only 和逐 specifier 信息；区分 default/named/namespace/TS equals，保存 imported/local 名称及各自 span。ExportIR 保留 default expression/declaration、声明导出、named、star、TS assignment/namespace 及 re-export source。

DeclarationIR 记录种类、绑定名称、span、逐变量 declarators 和顶层 inline/default 导出语法标记。VariableDeclaratorIR 保留 pattern、initializer，递归模式区分别名、默认值、computed key、rest 和 holes。exported 不是最终符号导出状态：独立 export { name } 与 namespace 内部导出应结合 ExportIR 和模块绑定解释。

SemanticIR 的 ScopeId/SymbolId/ReferenceId 都是模块内快照索引。作用域区分模块、函数、块、类、循环、catch、namespace 和类型；符号区分值/类型空间，引用区分读、写、读写、调用、构造、组件、类型和导出。内部 SWC resolver 提供语言绑定，公开类型不含 SWC context。JSX 名称不套用 React 大小写规则。未知环境全局保持未绑定，不默认报错。

ModuleBindingsIR 将导入与导出关联到原模块 Symbol、默认 Expression、匿名 Declaration、Namespace 或 External。ESM 星号重导出使用固定点处理循环，同一目标多路径去重，不同目标为歧义；显式导出覆盖星号。external 导出只能作为未验证引用，无法枚举的 external 星号另行记录。

ModuleGraph 记录规范文件路径、静态边、边种类和来源 span；External 是明确留给调用方的依赖，可表示为字符串的未解析目标以 None 加诊断表示。非法 UTF-16 说明符不生成图边，但原始 ImportIR/ExportIR、源码和诊断保留；含错误的图不能当作完整有效依赖图。ESM 强连通分量用于记录循环，运行期 TDZ 与类型语义不在首版图分析范围。

## 消费与演进

序列化不能丢弃 source，否则无法恢复精确 raw 和未知表达式。客户端投影必须由 Kenvoy 另建，不能直接把含源码与私有模块路径的整个 Generic IR 发到浏览器。

格式破坏性变化递增 IR 版本并提供明确拒绝或迁移路径；不能用新的 SWC AST 版本偷偷改变公共 JSON 结构。完整类型检查、业务静态求值、Schema、路由、HTTP、数据库和 React 均不属于 Generic IR。

阶段 02 新增字段使用 serde 默认值以读取原 v1 快照：缺失 shape 为 Opaque、declarators/semantic/module_bindings 为空。旧快照不会被伪装成已完成语义分析；依赖这些能力的调用方应重新分析源码。

相关接口见 [KEngine API](kengine-api.md)，设计原因见 [ADR 0003](../decisions/0003-generic-ir-design.md)，真实结果见 [验证索引](../verification/README.md)。
