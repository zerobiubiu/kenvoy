# 可运行示例

五个 kenvoy 项目均使用 **项目格式 0.4.0**，要求 **Kenvoy CLI 0.4.0 或以上兼容版本**。原生国际化始终启用：每个示例根目录都包含 `translations.json` 和按 `[languages]` 配置生成的 `language.schema.json`。

| 示例 | 内容 | 运行入口 |
|---|---|---|
| [customer-app](customer-app/kenvoy.toml) | 内置 DSL 客户登记，基础应用不需要 Node | 在示例目录执行 `kenvoy dev` |
| [extensions-app](extensions-app/README.md) | Schema 组合、自定义 React/Radix、第三方包与异步服务端动作 | 首次执行 `kenvoy dev --install-deps` |
| [options-app](options-app/README.md) | 静态/远程联动 select、系统重载、统一数据绑定 | 首次执行 `kenvoy dev --install-deps` |
| [uploads-app](uploads-app/README.md) | 多文件上传、图片预览/下载、URL 图片与受控 SDK 图库 | 首次执行 `kenvoy dev --install-deps` |
| [i18n-app](i18n-app/README.md) | 集中中英文译文、语言代码/别名混用、DSL 自动词条、语言切换、SDK 插值/复数/格式化与正式迁移 | 执行 `kenvoy dev`，SDK 演示需 Node/npm，无项目依赖 |

前四个示例显式只启用 `zh-CN`，`translations.json` 仅含 `"$schema": "./language.schema.json"`，界面沿用 DSL 默认文案和框架内置中文；这不是关闭国际化。`i18n-app` 显式启用中文和英文，每个业务词条完整提供两种语言。

在任一示例目录执行 `kenvoy i18n` 会更新 Schema、仅在译文文件不存在时创建只含 Schema 引用的文件，然后严格校验译文；不会提取源码、自动翻译或覆盖已有词条。新增词条必须覆盖配置中的全部语言，每种语言的代码与别名恰好选一个。`dev`、`run` 启动前同样更新 Schema 并校验；`check`、`build` 只读校验。

编辑器类型支持需要 dev 源码构建的 CLI：先在示例目录执行 `kenvoy types`，按报告补齐已有配置，再显式安装报告中的开发类型依赖并执行 `npm run typecheck`。没有配置时命令创建默认文件；已有第三方依赖清单不会被覆盖。不要另外安装 React 运行包或空壳 `@kenvoy/client`。详见[编辑器操作](../docs/operations/cli-design.md#编辑器类型)；此步骤只影响编辑支持，不改变示例业务库。


各示例分别使用各自 `data/app.sqlite`，默认均监听 3000 端口；同时运行时修改其中一个示例的端口。详细文档入口是 [docs/README.md](../docs/README.md)。
