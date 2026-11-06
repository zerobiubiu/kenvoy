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

五个示例统一使用 **Bun 1.4.3**，package.json 中固定 `packageManager: bun@1.4.3`。SDK、React JSX 与 DSL 类型改由 `@kenvoy/types` 开发依赖提供，不再执行 `kenvoy types` 或依赖 `.kenvoy/types`。不要安装空壳 `@kenvoy/client` 或重复 React 运行包。

仓库示例的开发依赖指向 `file:../../crates/kenvoy/frontend/dist/kenvoy-types-<CLI版本>-<内容摘要>.tgz`，消费实际 npm tarball，不依赖 Bun 的目录复制或链接。包内版本直接取 CLI Cargo 元数据，类型构建自动同步五例该开发依赖；SDK 内容或 CLI 版本改变都会换路径，避免 Bun 复用同一路径的旧包。构建产物位置记录于 `frontend/dist/types-artifact.json`，正式 Release 仍使用带版本号、不带本地摘要的资产名。在仓库根目录先生成类型包：

```text
npm --prefix crates/kenvoy/frontend run build:types
```

这仍是 CLI 内部前端工具链的 npm 命令，不改变示例使用 Bun 的约定。随后在需要使用的示例目录执行：

```text
bun install --ignore-scripts --omit=peer
bun run typecheck
```

本机 Bun 1.4.3 在并行首次安装同一归档时出现过缓存目录 ENOENT；多个示例按目录串行安装。不要改回固定 tarball 文件名：实测替换同一路径包内版本后，普通安装和 `--force` 仍保留旧包。

`tsconfig.json` 使用 `types: ["@kenvoy/types"]`，保留 `jsx: "react-jsx"` 和 `jsxImportSource: "@kenvoy/client"`，不再配置指向本地声明缓存的三个 `paths`。旧的本机缓存即使保留也不参与这套类型解析。

示例依赖锁文件只在本地生成和使用，不提交 Git；保留 package.json 中的依赖声明。没有共享锁文件时，各次安装解析出的间接依赖可能不同。`node_modules`、构建输出和示例锁文件均不入库；没有本地锁时不要使用 `--frozen-lockfile`，已有本地锁也不会因停止 Git 跟踪而删除。

自定义客户端构建仍需 Node.js 24.22+ 与其配套 npm，由 CLI 管理框架缓存。示例先用 Bun 安装依赖，再运行 `kenvoy dev/build/run`，不要使用当前仍走 npm 的 `--install-deps` 重新生成 npm 锁。当前 CLI 尚未把 bun.lock 纳入构建输入指纹，仅更新 Bun 锁后请显式执行 `kenvoy build` 再使用 run，避免复用旧资产。

旧声明生成与 npm 类型检查事实保留在 [0039](../docs/changes/0039-example-editor-types.md)、[0040](../docs/changes/0040-example-typecheck-dependencies.md)；[0041](../docs/changes/0041-examples-bun-package-manager.md) 中提交锁文件是当时记录，现按用户要求不再跟踪。类型包切换见 [0042](../docs/changes/0042-published-editor-types-package.md)。



各示例分别使用各自 `data/app.sqlite`，默认均监听 3000 端口；同时运行时修改其中一个示例的端口。详细文档入口是 [docs/README.md](../docs/README.md)。
