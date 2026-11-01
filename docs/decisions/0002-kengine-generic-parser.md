# 0002：通过 SWC 建立通用解析层

- 状态：参考；阶段 00 设计记录，SWC 具体依赖组合和解析探针待验证。
- 影响范围：KEngine Parser、AST 转换和诊断。

## 背景

用户源码包含真实 TypeScript、TSX、JSX、函数、泛型和跨文件模块。项目不实现完整 JavaScript Parser 或 TypeScript 类型检查器。

## 问题

解析层需要保留业务解释所需的信息，同时不能赋予 `page`、`resource` 等 JSX 名称任何业务含义。

## 可选方案

1. 正则或自制 DSL Parser：实现少，但不能可靠覆盖真实 TSX。
2. 直接执行用户 TSX 获取对象：失去静态边界，并引入执行副作用。
3. 使用 SWC 解析语法，再转换为业务无关 IR。

## 最终选择

KEngine 使用 SWC 的 ECMAScript/TypeScript Parser；按文件类型设置 JSX/TSX 语法选项，内部使用 SWC AST，公开结果使用自有 Generic IR。Parser 错误和可恢复错误均进入结构化诊断，不能只判断主解析调用是否返回成功。SWC 官方资料明确描述其部分语法错误恢复行为。[SWC Parser 文档](https://docs.rs/swc_ecma_parser/latest/swc_ecma_parser/)

模块解析与符号分析是独立的 KEngine 能力，不假定 SWC Parser 已自动完成 npm 依赖解析或完整类型检查。TypeScript 转 JavaScript 的执行流水线归 Kenvoy；不因复用 SWC 技术而移动职责。

## 原因

成熟 Parser 降低语言覆盖风险，独立转换层使上层不绑定底层 AST 的类型与版本。

## 已知缺点

SWC 子 Crate 的版本需要配套验证；源码位置、语法糖和表达式转换都有维护成本。解析成功不代表 DSL 合法，也不代表 TypeScript 类型正确。

## 影响

KEngine 需要在 `parse_source`、`parse_file` 和项目分析入口中统一诊断与位置语义。无法求值的表达式保留源码范围、类型类别及符号引用，交由调用者决定如何处理。

## 验证与状态

本轮查阅官方 API，未新增 SWC 依赖，未运行真实 TSX 解析探针。阶段 01 锁定配套版本后，必须覆盖嵌套 JSX、Fragment、Spread、泛型、Unicode 位置、语法错误和恢复错误；测试不得引入 Kenvoy 业务语义。
