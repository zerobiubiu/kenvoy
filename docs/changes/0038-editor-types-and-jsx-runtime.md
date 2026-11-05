# 0038：编辑器 SDK 类型与 JSX Runtime

状态：已执行；范围：本地 dev 源码、真实 TypeScript 正反向检查、CLI 与浏览器验收；未推送、未发布。前置记录：[0037：翻译引用预检与 React 函数模板](0037-i18n-reference-check-and-react-templates.md)。本轮不更改产品版本、项目格式或已有版本标签。

## 背景

用户在 kenvoy 项目编辑器中报告四类诊断：找不到 `react`、找不到 `@kenvoy/client`、找不到 `react/jsx-runtime`，以及没有 `JSX.IntrinsicElements`。

现有运行链由 Kenvoy 共享 React 并在构建阶段映射虚拟 SDK，但 create 没有交付编辑器配置和 SDK 声明。单独安装 React 不能解决 Kenvoy SDK 或 DSL 标签类型；写空模块、任意标签 any 索引、关闭 strict 或 skipLibCheck 都不能作为修复。

## 方案与行为

### 真实声明随 CLI 交付

新增 `frontend/tsconfig.types.json`，以真实 `src/sdk.ts`、生产和开发 JSX runtime 为根，执行 strict 的 emitDeclarationOnly，输出 `frontend/dist/types` 的完整传递声明闭包。前端 build 顺序改为 Vite 构建后生成声明，不手工复制 SDK API。

Cargo build.rs 要求 SDK 与两个 JSX runtime 声明入口齐全，将全部声明内嵌为 `frontend::EDITOR_TYPES`。Cargo 源包清单同时包含该配置和生成声明；声明不进入 `BUNDLED_ASSETS` 或应用 HTTP 静态目录，基础 create/types 不要求用户机安装 Node。

### 独立 JSX 类型命名空间

用户配置使用 `jsx: react-jsx` 与 `jsxImportSource: @kenvoy/client`。生产/开发桥接转发真实 React 的 jsx/jsxs/jsxDEV/Fragment，同时导出独立的纯类型 JSX 命名空间。

- app/page/resource、字段、option、form/table/image、action、导航、分组和重载有具体 DSL 属性。
- text、textarea、select、option、form、table、image 等同名标签，在 DSL 与原生 React HTML/SVG 属性间择一匹配，不能混入另一分支的专用属性。
- 保留事件、aria、children、ref、自定义组件 props 和真实 SDK 签名，不全局增补 React.JSX，不引入任意标签 any 索引。
- TypeScript 仍不替代 Kenvoy 的静态 Schema、值域、权限及服务端验证。

### create 与 kenvoy types

create 默认生成私有 package.json、tsconfig.json 及 `.kenvoy/types`，保留既有初始迁移流程，不创建 SQLite、不安装依赖。旧项目或清理缓存后可执行：

```text
kenvoy types
```

该命令不编译/执行项目源码，不读取译文内容，不访问业务库，不调用 npm。缺少两项配置时创建；已有文件，包括损坏 JSON 或非 UTF-8 字节，都保留原始内容并明确报告需手工核对。

报告状态为 `declarations_generated`，不把“已生成声明”冒充“依赖及项目类型已就绪”。报告包含配置 created/preserved 状态、`configuration_review_required`、完整接线建议和显式安装/类型检查命令。

默认 TypeScript 配置使用 ES2022、DOM、ESNext、Bundler、strict、noEmit、allowJs、allowImportingTsExtensions；覆盖项目内 JS/TS/JSX/TSX，排除 `.kenvoy`、node_modules、迁移、构建输出及持久化目录。三项 paths 指向：

| 模块 | 声明入口 |
|---|---|
| `@kenvoy/client` | `./.kenvoy/types/sdk.d.ts` |
| `@kenvoy/client/jsx-runtime` | `./.kenvoy/types/presentation/jsx-runtime.d.ts` |
| `@kenvoy/client/jsx-dev-runtime` | `./.kenvoy/types/presentation/jsx-dev-runtime.d.ts` |

私有 package 的 typecheck 脚本执行 `tsc --noEmit`，仅声明 `typescript`、`@types/react`、`@types/react-dom` 开发依赖，版本从框架 package.json 读取。本轮实际版本为 7.0.2、19.3.0、19.3.0；不添加运行时 React 或占位 `@kenvoy/client`。

声明缓存用摘要清单接管自己的文件，完整预检后原子更新，支持中断恢复；拒绝越界、链接、未知同名文件、损坏清单和修改过的当前声明。只移除摘要仍匹配的过期声明，额外用户文件及被用户接管的旧文件保留。

### 共享构建与旧缓存升级

生产/开发共用 client-builder，识别 `@kenvoy/client/jsx-runtime` 和 `jsx-dev-runtime` 并映射至框架桥接，仍共享同一个 React。

前端 package 的 build 脚本变更会改变内嵌清单。原工具链目录只按锁文件摘要分组，会与旧清单产生 immutable 冲突；现改为 package＋lock 联合摘要，旧缓存保留，新清单建立独立目录，不覆盖或清理旧版本。

## 实施文件

| 职责 | 文件 |
|---|---|
| 类型与 JSX 桥接 | `crates/kenvoy/frontend/src/presentation/{dsl-types.ts,jsx-types.ts,jsx-runtime.ts,jsx-dev-runtime.ts}` |
| 声明构建与源包 | `crates/kenvoy/frontend/{package.json,tsconfig.types.json}`、`crates/kenvoy/{build.rs,Cargo.toml}` |
| 客户端集成 | `crates/kenvoy/frontend/scripts/client-builder.mjs`、`crates/kenvoy/src/infrastructure/client_build/{sources.rs,build.rs}` |
| 项目生成与保护 | `crates/kenvoy/src/infrastructure/project/editor/{mod.rs,config.rs,files.rs,cache.rs,tests.rs}`、`project/{mod.rs,create.rs}` |
| CLI | `crates/kenvoy/src/bootstrap/commands.rs`、`interfaces/cli/{args.rs,output.rs}` |
| 消费者与保护回归 | `crates/kenvoy/frontend/tests/{editor-types.test.mjs,client-builder.test.mjs,types/}`、`crates/kenvoy/tests/{types_cli.rs,types_cli/}`、`project/i18n/tests.rs` |
| 文档 | 仓库与 Crate README、示例入口、CLI/SDK/前端架构/工程流程、文档中心、CHANGELOG、变更及验证索引和本记录 |

## 回归中的修正

初次集成发现 sha2 0.11 的摘要数组没有 LowerHex 实现，不能直接 `format!("{:x}", digest)`；生产代码及夹具改为预分配 String 后逐字节编码。原 i18n 模板测试中“create 不生成 package.json”的偶然默认断言已删除，不改为反向锁定新默认。只测试声明复制、默认 JSON 或版本转发的新增断言也移除，由真实 TypeScript 消费者证明类型有效。

Windows junction 测试初次在 cmd/mklink 阶段把路径中的正斜杠识别为开关；修正夹具为本机反斜杠后实际链接拒绝场景通过，没有跳过或放宽链接保护。

## 执行验证记录

### 环境与自动回归

实际环境：Windows x64，Rust/Cargo 1.99.1，Node v24.22.0，npm 12.2.0；沿用仓库精确锁定依赖。工程验证只在自有临时应用显式安装开发类型，没有给已有示例或用户应用静默安装依赖。

| 命令/场景 | 实际结果 | 结论 |
|---|---|---|
| `npm run check && npm test && npm run build`，工作目录 `crates/kenvoy/frontend` | tsc 退出 0；144 passed、0 failed；Vite 129 modules；SDK 声明生成完成；整体退出 0 | 前端类型、真实声明消费、自动 JSX 和生产构建通过 |
| `cargo fmt --all && cargo xtask test`，最终完整运行 | Kenvoy 单元 228 passed、0 failed、1 ignored；types_cli 9 passed；其余 Workspace 非 ignored 回归通过；整体退出 0 | create/types、旧配置保护、缓存升级/恢复、路径/链接及既有跨层行为通过 |
| `cargo xtask architecture` | 四成员、`kenvoy -> kengine`；40 个前端模块、466 个自有源码，最大 449 行，上限 500；退出 0 | 分层、依赖和规模门禁通过 |
| `cargo package -p kenvoy --list --allow-dirty --offline --locked` | 清单列出 tsconfig.types.json、sdk.d.ts、两个 JSX runtime 与传递声明；退出 0 | 源包包含构建所需声明；此命令未发布，也不证明独立源包编译 |

前端消费者回归实际调用 TypeScript，在没有 React 运行包、没有框架源码的临时应用中，只链接开发类型依赖，验证生产/开发 JSX 均可检查。反向素材逐条验证未知 SDK 导出、私有接口、错误 API/DSL 参数、未知标签、原生事件类型及 React 命名空间不被污染；没有使用 ts-expect-error 或只检查生成文本。

### 真实 CLI 与编辑器等价检查

先用改动前的 CLI 创建自有应用并完成真实自定义 React 构建，留下旧客户端工具链缓存。再用新 CLI 执行 types：

- 退出 0，`declarations.total = 24`，24 个声明首次生成。
- 两项配置为 created，`dependency_installation_performed = false`；报告要求显式 `npm install --ignore-scripts`。
- 在该临时应用显式执行 `npm install --offline --ignore-scripts --no-audit --no-fund && npm run typecheck`：安装 5 个包，tsc 退出 0。
- 实测应用根 node_modules 中没有 react、react-dom、@kenvoy/client 运行包，模块和 JSX 类型仍正确解析。

随后在 include 范围内放入普通文件名 `src/tmp-editor-control.tsx`，包含未知 SDK 导出、字符串赋给 number、给 useLocale 传参、数字 page.path 和未知标签。真实 `npm run typecheck -- --pretty false` 退出 1，分别产生 TS2305、TS2322、TS2554、TS2322、TS2339。删除控制文件后再次 typecheck 退出 0，证明并非空检查、any 或关闭诊断后假通过。

types_cli 的永久行为回归验证：没有 npm 仍能 create/types；不执行非法或缺失源码，不读取损坏译文，不触碰已有数据库或迁移；已有/损坏配置字节保持；只创建缺失的一项；幂等不改 mtime；生成文件冲突、恶意清单、目录替换及 Windows junction 都被拒绝且目标文件保持。

### 真实开发运行与浏览器

该应用的组件显式使用 `@jsxImportSource @kenvoy/client`，同时 import React useState、原生 useLocale，并含普通 SVG text。新 CLI dev 构建并监听成功：

- 旧工具链 `c3cc09b46adbbf97d1e4bfc3-win32-x64-node24` 保留；新清单使用独立 `6794caa88e08cad303859ccf-win32-x64-node24`，没有旧 package immutable 冲突。
- 使用 omp 当前默认浏览器配置打开真实页面，点击使“计数 0”变为“计数 1”。
- useLocale 显示 en-US；SVG 文本为 React SVG，实际命名空间为 `http://www.w3.org/2000/svg`；浏览器运行时错误集合为空，已查看截图。
- HTTP 访问 `/.kenvoy/types/sdk.d.ts` 返回 404，声明没有进入公开静态目录。

本地证据保存在忽略目录 `artifacts/editor-types-smoke/`：types 报告、TypeScript 正反向结果、旧/新缓存记录、browser-evidence.json 与 browser.webp。验收标签页、服务、自有临时项目及其数据库/node_modules/缓存均已清理；不把停止服务冒充原生 Ctrl+C 验证。

### 已知限制与后续验证建议

- 本轮验证真实 TypeScript 编译器与浏览器，没有打开某个特定版本的 VS Code；已有编辑器缓存可能仍需重新加载 TS 项目或重启 TS Server。
- 已有 tsconfig/package 不会自动迁移。尤其 extends、baseUrl、已有 paths、第三方 UI 包或非默认 JSX 编译设置，应先按报告手工合并，再执行自己的类型检查；不要整体覆盖用户配置。
- TypeScript 的 JSX 属性检查不代替 Rust 的静态 Schema、路由、权限和运行时数据校验。服务端和基础 DSL 不因编辑器类型交付而要求 Node。
- 现有 ignored 浏览器/显式用户依赖套件没有执行；本轮浏览器结论仅覆盖上述真实开发场景，未新增 Linux/macOS 编辑器结论。
- 本轮 rust-analyzer 启动即退出，LSP 引用查询不可用；依据源码结构、模块边界、真实 tsc 和跨层回归验证。目录中 advertised ego-browser skill 无法加载，已报告工具问题；实际使用 omp 默认 browser 能力，未修改浏览器配置。
- 后续发布前按批准版本更新发布与兼容说明；当前类型入口和命令尚未包含在已发布的 0.4.4 中。CLI 升级后重新运行 types，并显式安装匹配开发类型即可刷新编辑器交付。
