# 阶段 00 前端兼容性探针

本目录是独立技术验证，不是正式 Kenvoy 前端 Runtime，不新增 Rust Crate。JSON 描述由探针提供，不是 KEngine 解析输出；表单明确提示不连接 API 或 SQLite。

## 已验证范围

2026-11-01 在 Windows、Node v24.22.0、npm 12.2.0 和已有 Chromium 156.0.8078.4 上执行：

- JSON 页面描述通过注册表渲染 React 原生字段；真实浏览器完成输入和表单交互。
- React Router 支持构建产物的 `/customers` 深层地址。
- 跨文件普通组件支持 Props、Hooks 与 CSS。
- Vite/Rolldown 生产构建及 manifest/source map 生成成功。
- 本次函数组件 JSX 文本修改后计数器值 3 保留；CSS 修改后颜色更新且计数仍为 3。
- 故意写入语法错误后出现真实错误遮罩；修复后恢复组件显示。
- 客户端构建的 HTML/JS/CSS/JSON/source map 未包含测试 server 标记、路径或函数名；显式导入 `.server.ts` 的负例被本探针插件拒绝。

`npm run probe` 的最终实际输出为 `结果：passed`、退出码 0；详细结果与截图写入忽略目录 `.probe/`。测试本身会写入无效代码并恢复，预期的负例会在终端打印 Build failed 或语法错误，不代表总验收失败。

## 版本与安装

精确版本写入 `package.json`，解析结果、来源和 integrity 写入 `package-lock.json`：React/React DOM 19.3.1、React Router 8.5.1、Vite 8.3.5、官方 React 插件 6.1.2、Rolldown 1.2.16、Playwright Test 1.64.1。

本次首先查询 npm registry 的稳定 dist-tag、peer dependencies 和 engines，随后执行锁定安装；没有使用主分支版本作为锁定依据。初次安装产生一项 optional peer 的 extraneous `yaml`，执行 `npm prune --ignore-scripts --no-audit --no-fund` 后移除；再执行 `npm ci --ignore-scripts --no-audit --no-fund` 安装 105 个包，`npm ls --depth=0` 仅列出上述十个直接依赖。

本项目用户已授权工程必要依赖自主决定；本次安装处于该授权范围，并保留锁文件。探针启动不自动修改依赖，复现命令为：

```powershell
npm.cmd ci --ignore-scripts --no-audit --no-fund
npm.cmd run build
npm.cmd run probe
```

本次已有 Playwright 对应 Chromium，无需下载。后续若缺少浏览器，在本工程授权范围内显式准备并记录版本，不由测试脚本每次启动时自动下载。

## Axum 同端口跨工具验证

补充脚本 `scripts/axum-browser.mjs` 由显式运行的 Rust ignored test 调用；脚本不启动替代 HTTP 服务。Axum 在 loopback 同端口托管本目录真实 `dist` 与 SQLite API。Chromium 验证三个 React 字段和 Hooks 计数器交互后，独立同源 `fetch` 创建 `{name: 'Axum浏览器', age: 25}`，收到 HTTP 201 和真实记录；无凭据列表请求为 401，未知资源为 404，错误响应不是 HTML。Rust 关闭服务后重开数据库，确认 1 条记录。

实际命令退出码 0：

```powershell
cargo test -p kenvoy --test p03_http_sqlite axum_serves_built_react_and_sqlite_to_real_chromium --locked -- --ignored --nocapture
```

该命令从仓库根目录执行，前提是本目录已构建且 Playwright/Chromium 已准备；普通 `cargo test` 不运行该跨工具测试。记录见 [Axum/React/SQLite 浏览器报告](../../../docs/verification/0002-axum-react-browser.md)。

## 生命周期与首次失败

首轮探针先在同一 Node 进程调用生产 `build`，再调用 `createServer`；Vite 保留了 `NODE_ENV=production`，React 插件因此关闭 Fast Refresh，文件变化导致整页刷新，计数从 3 变为 0，总探针退出码 1。此结果保留在本次 `.probe/attempt-01-build-env-failure.json`。

修正为开发服务启动前显式设置 `NODE_ENV=development`，并在 `finally` 恢复原环境。没有移除断言或更换依赖；再次运行后观察到 `hmr update /src/CustomerCard.tsx` 和计数值保留。最终 `finally` 恢复被修改的探针源码，关闭浏览器、开发服务和生产静态服务。

## 验证边界

- `npm run probe` 的独立生产静态服务是 Node HTTP 测试服务；补充跨工具测试已另外验证 Axum 同端口托管真实 React 资产和 SQLite API。
- 本次 `.server.ts` 边界来自探针配置中的显式插件，不代表完整 Kenvoy Client/Server/Schema/Shared 模块隔离已经完成。
- Fast Refresh 结论只覆盖本次函数组件、导出形态和编辑序列；不承诺任意组件、混合导出或 Schema 更新保留状态。
- 表单交互不证明前后端校验一致、API 安全、管理员认证或数据持久化。
- TSX 转换成功不代表 TypeScript 完整类型检查通过。

## 依据

设计和后续正式适配参考 [Rolldown 能力范围](https://rolldown.rs/guide/introduction)、[Vite 后端集成](https://vite.dev/guide/backend-integration)、[官方 React 插件](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react/README.md) 及 [React 受控字段](https://react.dev/reference/react-dom/components/select)。本目录保留显式配置仅便于独立验证；正式用户项目不应被要求手写 Vite 配置。
