# 可运行示例

| 示例 | 内容 | 运行入口 |
|---|---|---|
| [customer-app](customer-app/kenvoy.toml) | 内置 DSL 客户登记，基础应用不需要 Node | 在示例目录执行 `kenvoy dev` |
| [extensions-app](extensions-app/README.md) | Schema 组合、自定义 React/Radix、第三方包与异步服务端动作 | 首次执行 `kenvoy dev --install-deps` |
| [options-app](options-app/README.md) | 静态/远程联动 select、系统重载、统一数据绑定 | 首次执行 `kenvoy dev --install-deps` |
| [uploads-app](uploads-app/README.md) | 多文件上传、图片预览/下载、URL 图片与受控 SDK 图库 | 首次执行 `kenvoy dev --install-deps` |
| [i18n-app](i18n-app/README.md) | 原生中英文、DSL 自动词条、语言切换、SDK 插值/复数/格式化与正式迁移 | Kenvoy CLI 0.3.0；执行 `kenvoy dev`，SDK 演示需 Node/npm，无项目依赖 |

各示例分别使用各自 `data/app.sqlite`，默认均监听 3000 端口；同时运行时修改其中一个示例的端口。详细文档入口是 [docs/README.md](../docs/README.md)。
