# 可运行示例

五个 kenvoy 项目均使用 **项目格式 0.4.0**。当前编辑器配置采用原生 SDK/JSX 类型入口，需要使用本仓库 dev 源码构建的 Kenvoy CLI；已发布的 0.4.4 尚不包含 types 命令及对应 JSX 入口。原生国际化始终启用：每个示例根目录都包含 `translations.json` 和按 `[languages]` 配置生成的 `language.schema.json`。

| 示例 | 内容 | 运行入口 |
|---|---|---|
| [customer-app](customer-app/kenvoy.toml) | 内置 DSL 客户登记，基础应用不需要 Node | 在示例目录执行 `kenvoy dev` |
| [extensions-app](extensions-app/README.md) | Schema 组合、自定义 React/Radix、第三方包与异步服务端动作 | 先用 Bun 安装项目依赖，再执行 `kenvoy dev` |
| [options-app](options-app/README.md) | 静态/远程联动 select、系统重载、统一数据绑定 | 先用 Bun 安装项目依赖，再执行 `kenvoy dev` |
| [uploads-app](uploads-app/README.md) | 多文件上传、图片预览/下载、URL 图片与受控 SDK 图库 | 先用 Bun 安装开发依赖，再执行 `kenvoy dev` |
| [i18n-app](i18n-app/README.md) | 集中中英文译文、语言代码/别名混用、DSL 自动词条、语言切换、SDK 插值/复数/格式化与正式迁移 | 先用 Bun 安装开发依赖，再执行 `kenvoy dev` |

前四个示例显式只启用 `zh-CN`，`translations.json` 仅含 `"$schema": "./language.schema.json"`，界面沿用 DSL 默认文案和框架内置中文；这不是关闭国际化。`i18n-app` 显式启用中文和英文，每个业务词条完整提供两种语言。

在任一示例目录执行 `kenvoy i18n` 会更新 Schema、仅在译文文件不存在时创建只含 Schema 引用的文件，然后严格校验译文；不会提取源码、自动翻译或覆盖已有词条。新增词条必须覆盖配置中的全部语言，每种语言的代码与别名恰好选一个。`dev`、`run` 启动前同样更新 Schema 并校验；`check`、`build` 只读校验。

五个示例统一使用 **Bun 1.4.3**，package.json 中固定 `packageManager: bun@1.4.3`。编辑器类型支持需要 dev 源码构建的 CLI；先在示例目录执行 `kenvoy types`，项目依赖和脚本使用 Bun 管理，不照搬 types 报告中面向默认 npm 安装器的命令。不要安装空壳 `@kenvoy/client`。

本轮已为全部示例生成编辑器类型；`.kenvoy/types` 是本机派生缓存，不随 Git 提交，重新克隆或清理缓存后再次执行 `kenvoy types`。

| 示例 | 先前生成的 SDK 声明 | 项目包管理器 | 本轮验证 |
|---|---:|---|---|
| customer-app | 24 | Bun 1.4.3 | 按要求未执行 |
| extensions-app | 24 | Bun 1.4.3 | 按要求未执行 |
| options-app | 24 | Bun 1.4.3 | 按要求未执行 |
| uploads-app | 24 | Bun 1.4.3 | 按要求未执行 |
| i18n-app | 24 | Bun 1.4.3 | 按要求未执行 |

示例的依赖锁文件只在本地生成和使用，不提交 Git；保留 package.json 中的依赖声明。没有共享锁文件时，各次安装解析出的间接依赖可能不同。后续使用时可在示例目录执行：

```text
kenvoy types
bun install --ignore-scripts --omit=peer
bun run typecheck
```

本地没有 bun.lock 时，Bun 安装过程可生成它；已有本地锁文件不会因停止 Git 跟踪而删除。`node_modules`、`.kenvoy/types` 和示例锁文件均不入库；没有本地锁时不要使用 `--frozen-lockfile`。

本轮只切换示例项目，不切换 CLI 内部的框架工具链。自定义客户端构建仍需 Node.js 24.22+ 与其配套 npm，由 CLI 管理框架缓存。示例先用 Bun 安装依赖，再运行 `kenvoy dev/build/run`，不要使用当前仍走 npm 的 `--install-deps` 重新生成 npm 锁。当前 CLI 尚未把 bun.lock 纳入构建输入指纹，仅更新 Bun 锁后请显式执行 `kenvoy build` 再使用 run，避免复用旧资产。

声明生成和此前 npm 类型检查事实分别见 [0039](../docs/changes/0039-example-editor-types.md)、[0040](../docs/changes/0040-example-typecheck-dependencies.md)；[0041](../docs/changes/0041-examples-bun-package-manager.md) 中提交锁文件的做法是当时的历史记录，现按用户要求不再跟踪示例锁文件。



各示例分别使用各自 `data/app.sqlite`，默认均监听 3000 端口；同时运行时修改其中一个示例的端口。详细文档入口是 [docs/README.md](../docs/README.md)。
