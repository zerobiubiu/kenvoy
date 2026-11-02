# KEngine

通用 JavaScript、TypeScript、JSX、TSX 源码解析与模块分析库，是 Kenvoy 使用的独立语言引擎。

## 安装

```toml
[dependencies]
kengine = "0.2.0"
```

需要 Rust 1.98 或以上，edition 2024。SWC 及其原生依赖需要对应平台的 Rust/C 构建环境。

## 能力与边界

- 解析源码，保留源码位置、UTF-16 字符串、表达式和通用 JSX 结构。
- 分析模块依赖、作用域、符号、引用及跨模块绑定。
- 使用 `AnalysisSession` 进行增量分析，区分候选结果与最后有效结果。
- 通过 `Diagnostic` 提供结构化诊断，不将恢复出的 AST 当成有效程序。

入口类型为 `Engine`；常用方法是 `parse_source`、`parse_file`、`resolve_module` 和 `analyze_project`。公开结果为自有 IR，不把 SWC 内部类型作为长期 API。

本库不执行用户代码、不解释页面/资源/数据库语义，也不是完整的 TypeScript 类型检查器。Web 宿主由独立的 `kenvoy` 包提供。

## 文档与源码

- [API 文档](https://docs.rs/kengine/0.2.0/kengine/)
- [源码及项目文档](https://github.com/zerobiubiu/kenvoy)
- [KEngine 契约](https://github.com/zerobiubiu/kenvoy/blob/v0.2.0/docs/contracts/kengine-api.md)

0.2.0 为当前通用语言引擎实现；不要沿用 0.1.0 占位包 README 中的 `hello`/`VERSION` 示例。

## 许可证

MIT，见随包提供的 LICENSE。
