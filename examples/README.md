# 可运行示例

| 示例 | 内容 | 运行入口 |
|---|---|---|
| [customer-app](customer-app/kenvoy.toml) | 内置 DSL 客户登记，基础应用不需要 Node | 在示例目录执行 `kenvoy dev` |
| [extensions-app](extensions-app/README.md) | Schema 组合、自定义 React/MUI、第三方包与异步服务端动作 | 首次执行 `kenvoy dev --install-deps` |

两个示例分别使用各自 `data/app.sqlite`，默认均监听 3000 端口；同时运行时修改其中一个示例的端口。详细文档入口是 [docs/README.md](../docs/README.md)。
