# 0003：版本化 Generic IR 与业务 IR 分离

- 状态：参考；阶段 00 设计记录，数据结构与序列化兼容性待探针验证。
- 影响范围：KEngine 公共 API、Generic IR、源码追踪。

## 背景

Kenvoy 是 KEngine 的首个调用方，但不是唯一可能的调用方。SWC AST 是解析实现细节，不应成为长期业务接口。

## 问题

纯 JSON 属性值无法表达函数、组件、Spread 和动态表达式；直接输出 SWC AST 又会把所有调用方绑定到其内部变更。

## 可选方案

1. 直接公开 SWC AST：信息丰富，但公共契约耦合底层实现。
2. 把 JSX 属性全部转成字符串或 JSON：简洁，但丢失语言语义。
3. 自有带版本 Generic IR，保留表达式引用与必要源码信息。

## 最终选择

采用方案 3。Generic IR 包括 Program、Module、Import、Export、Declaration、Symbol、Reference、Element、Fragment、Attribute、Spread、Expression 和 Source Location。属性值使用带区分标签的联合结构，区分字面量、布尔简写、表达式及 Spread。

所有跨节点关联采用稳定于当前分析快照的 ID；ID 不承诺跨任意源码修改永远不变。表达式至少保留种类、源码范围与引用关系；静态未知是显式结果，不能转换成空值后继续处理。源码位置明确区分字节范围与面向用户的行列。

序列化顶层包含格式版本；未知版本拒绝读取或通过明确适配器升级。Kenvoy 另行维护 Application IR 版本，不能用 Generic IR 版本代替业务协议版本。

## 原因

保留可分析信息并隔离 Parser 的版本变化；业务层可以做更严格的静态求值，而通用层无需知道字段、路由或数据库。[SWC Parser 与 AST 使用方式](https://docs.rs/swc_ecma_parser/latest/swc_ecma_parser/)

## 已知缺点

IR 转换有额外内存和测试成本。第一版不试图完整规范所有 ECMAScript 运行语义，未覆盖结构必须有可追踪表示或明确诊断。

## 影响

公开 API 不能泄漏必须由调用者理解的 SWC 内部类型。缓存须绑定内容摘要、分析选项、依赖图和 IR 版本，避免旧符号与新节点混用。

## 验证与状态

本轮没有实现或发布 IR。阶段 01/02 使用普通 React、JSX DSL、多模块、循环引用、函数和泛型样例验证信息保真与序列化往返；业务解释结果的测试归 Kenvoy。
