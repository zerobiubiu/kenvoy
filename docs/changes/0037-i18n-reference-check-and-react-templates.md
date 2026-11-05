# 0037：翻译引用预检与 React 函数模板

状态：已执行；范围：本地 dev 源码、自动回归与隔离运行验收，未推送、未发布。前置记录：[0028：强制原生语言、集中译文与 Schema](0028-mandatory-languages-and-schema.md)。

本次收紧显式业务翻译调用的缺词行为；自动 DSL 默认文字、语言矩阵、Application IR、迁移协议及既有 0.4.4 发行事实保持。未安排新的产品版本，也未移动已有标签。当前契约见[原生语言](../contracts/i18n.md)、[声明语法](../contracts/dsl-specification.md)。

## 背景

用户要求三项能力：kenvoy 项目启动前检查源码调用的翻译字段，缺失则先报错；提供默认系统函数读取当前语言；默认入口支持 React 模板语法。

原实现仅验证集中译文自身的语言矩阵。实际隔离 CLI 场景中，组件调用 `t('business.unconfigured')` 而根词典没有该键，旧 Binary 的 `check` 仍退出 0。`export default function App(){ return <app .../>; }` 则被旧实现以 `KV1101` 拒绝。原有 `useI18n().locale` 已有响应式语言状态，本次不建立第二套状态。

## 方案与实现

### 源码翻译预检

新增 `javascript::validate_i18n(&ProgramIR, &I18nBundle)`，复用 KEngine 的模块图、词法绑定、通用表达式和源码位置；SWC 适配器补充调用、可选链和逗号表达式结构，不执行项目代码。

- 沿 SDK 导入、别名、命名转导、namespace、解构、`i18n.t` 和 const 别名追踪翻译身份；普通同名函数、局部遮蔽、注释及字符串不算翻译调用。
- 检查静态字符串、局部/跨模块字符串 const、简单拼接/模板和有限条件键的完整集合。未决动态键、普通对象/数组查表键、未知调用透传或容器逃逸以 `KV2202` 阻断，不能默默跳过。
- 缺失业务键或语言覆盖以 `KV2201` 报错，包含文件、行列、键名及补齐 `translations.json` 的建议；第三个 fallback 参数不能豁免。
- 直接复用前端真实中英文 UI/错误 JSON 的键，以 `LazyLock` 初始化一次系统键集合，不维护另一套手写清单。
- 复数调用接受完整原始键或 `.other`，仅有 `.one` 不足够；无 count 时不能用 `.other` 代替原始键。
- 源码或系统词典分析失败为 `KV2203`。不自动补键、不把中文复制为其他语言、不改写用户译文。

`build::candidate` 在静态声明分析后、服务端脚本准备与客户端构建之前调用检查器，并复用 staging 已装载的规范词典。`check` 使用同一检查器；`dev` 初次启动和候选更新复用 candidate 路径。`run` 的数据库快照检查移到候选/产物校验之后，避免数据库读取错误抢先掩盖源码缺词诊断。

无源码运行继续只接受与当前 CLI 指纹、配置及根译文匹配的已验证产物；不能重新扫描不存在的源码，也不能用根译文变更冒充已构建产物。自动生成的 DSL 标签键仍允许使用源码默认文字，不等同于显式 `t(...)` 调用。

### 原生当前语言函数

`@kenvoy/client` 新增 `useLocale(): string`，直接读取既有 Runtime i18n 上下文。返回 `zh-CN`、`en-US` 等规范代码，不返回 alias/display；与 `useI18n().locale` 同步响应页面内切换。

它是 React Hook：在普通 React 组件或自定义 Hook 顶层使用，不提供组件外的全局可变语言单例，不在 Schema 提取函数中调用。现有设备语言选择、别名解析、草稿、权限和数据库迁移位置不变。

### 静态 React 函数入口

在 Kenvoy 应用层新增 `FunctionTemplateParser` 端口及 `FunctionTemplate`，由现有 SWC 适配器实现；不修改 KEngine，不把 React 或翻译业务语义放进通用解析引擎。

支持命名/匿名函数、函数表达式、表达式/块箭头函数、括号/TypeScript 包装、静态函数别名及既有模块导入/重导出。函数内可按顺序声明静态 const，组合 JSX/Fragment，再由一个直接 return 返回声明树。

- 不执行函数；拒绝参数、async/generator、Hooks、运行时初始化、控制流或嵌套函数生成 Schema。
- 最终仍须是单个 app/page 根；普通 React UI 的 Hooks、事件、条件渲染和列表 map 仍在客户端执行。
- `.diu.tsx` 保持既有静态展开契约，不改为执行函数式 Schema Component。
- 客户端切片只捕获经模板语法验证的局部 const，通过局部 IIFE 保留遮蔽与闭包；原私有依赖检查和求值预算保持。

## 实施文件

以下按职责归组；各路径中的花括号表示本次修改文件集合，不是运行路径。

| 职责 | 文件 |
|---|---|
| 启动与构建接入 | `crates/kenvoy/src/bootstrap/{build.rs,commands.rs}`、`infrastructure/artifacts/staging.rs` |
| 翻译检查器 | `crates/kenvoy/src/infrastructure/javascript/compiler/i18n/{mod.rs,source.rs,bindings.rs,links.rs,values.rs,plural.rs,tests.rs}` |
| SWC 与导出 | `crates/kenvoy/src/infrastructure/javascript/{mod.rs,compiler/mod.rs,compiler/parser.rs,compiler/templates.rs}` |
| 静态模板提取 | `crates/kenvoy/src/application/compilation/static_eval/{mod.rs,evaluator.rs,expressions.rs,targets.rs,templates.rs}` |
| 分析与闭包 | `crates/kenvoy/src/application/compilation/analyzer/{mod.rs,tests/mod.rs,tests/templates.rs}`、`client_slice/{mod.rs,collector.rs,tests.rs}` |
| 求值回归迁移 | `crates/kenvoy/src/application/compilation/static_eval/tests/{mod.rs,evaluation.rs,references.rs,restrictions.rs,function_templates.rs,template_restrictions.rs}` |
| 跨层回归 | `crates/kenvoy-tests/tests/native_i18n.rs`、`i18n_support/references.rs` |
| 测试夹具隔离 | 上述 analyzer/static_eval 测试 helper，以及 `crates/kenvoy/src/infrastructure/persistence/sqlite/migration/catalog/tests/mod.rs` |
| SDK 与示例 | `crates/kenvoy/frontend/src/{sdk.ts,presentation/i18n.tsx}`、`examples/i18n-app/{src/app.tsx,src/I18nDemo.tsx,README.md}` |
| 文档 | `docs/README.md`、`contracts/{README.md,i18n.md,dsl-specification.md}`、`changes/{README.md,CHANGELOG.md}`、`verification/README.md` 和本记录 |

## 回归期间发现的问题

首次集成暴露了 IR 标识符没有 Hash、Evaluator 新端口未接入一个调用方、SWC 30 的 FunctionBody/ArrowFunctionBody API 差异；已按既有 Ord 标识使用 BTree 索引、接入解析器端口并采用真实 SWC 类型，不改 KEngine API。

新增模板正向夹具曾用局部变量 `page` 遮蔽内置 `<page>`，与既有“已绑定小写名称不能冒充内置标签”规则冲突；改为 `pageTree`，没有放宽产品规则。并行回归还暴露三个旧 helper 仅用 PID 与系统时间命名目录，出现 AlreadyExists/NotFound；加入进程内原子序号隔离，保持默认并行运行，不靠串行化避开问题。

新跨层回归发现 `run` 原先先读取数据库快照，既有异常数据库会先报 `file is not a database` 而非缺词诊断；调整上述启动顺序后，缺词对新库和已有文件都先报 `KV2201`，译文、迁移清单和数据库字节不变。

最终安全回归又发现：通用 Schema 求值器的对象初始快照不能证明运行时容器不变。`const keys={title:'known'}` 在组件内被直接或经函数改为 missing 后，原检查仍会放行。保留失败前用例，删除这条不安全的求值回退及其冗余状态，普通对象/数组查表键现在明确报 `KV2202`；没有为此增加通用对象变更分析框架。

## 执行验证记录

### 环境与命令

实际环境：Windows x64；`rustc 1.99.1`、`cargo 1.99.1`、Node `v24.22.0`、npm `12.2.0`。沿用仓库锁文件与已有工具，未安装用户应用依赖，未向外部发布。

| 命令或场景 | 实际输出与退出码 | 结论 |
|---|---|---|
| `npm run check && npm test && npm run build`，工作目录 `crates/kenvoy/frontend` | tsc 通过；143 passed、0 failed；Vite 构建 129 modules；退出 0 | 原生 SDK 类型、既有前端行为与发行资产构建通过 |
| `cargo xtask test`，最终完整运行 | 默认并行回归完成；Kenvoy 单元 223 passed、0 failed、1 ignored；native_i18n 12 passed、0 failed；xtask 31 passed；整体退出 0 | 全 Workspace 非 ignored 用例通过；不将 0 tests 或 ignored 视作运行证明 |
| `cargo xtask architecture` | `status: 通过`；四成员及 `kenvoy -> kengine`；450 个自有源码，最大 449 行，上限 500；退出 0 | 分层、依赖和规模门禁通过 |
| `cargo fmt --all && cargo fmt --all -- --check` | 无输出，退出 0 | 最终格式通过 |
| `cargo xtask docs` | 109 篇 Markdown、1189 个本地链接，`status: 通过`；退出 0 | 文档索引、链接与文本门禁通过 |
| 自有临时项目，旧 Binary `check` | 未配置 `business.unconfigured`，退出 0 | 复现原缺口 |
| 同一输入，新 Binary `check` | `KV2201`，指向 `MissingTranslation.tsx` 第 4 行，第 17 列；退出 1；数据库不存在 | 源码缺词在执行前被拒绝 |
| 补齐中/英/韩集中译文，函数式 App 入口 `check` | 1 个资源、1 个页面，退出 0 | 词典修复与函数模板跨层恢复 |
| 正式 `examples/i18n-app` 的只读 `check` | 1 个资源、2 个页面，退出 0 | 已更新示例可分析，不打开示例数据库 |
| 自有临时项目真实 `run` | 构建并监听成功；`/customers` HTTP 200；描述含规范语言与三个 business 词条 | 非测试替身的构建、HTTP 与客户端资产链路通过 |
| 可变对象键安全回归与真实 CLI | 新回归修复前 1 failed，修复后随完整套件通过；真实 CLI 对 `t(keys.title)` 报 `KV2202`，定位第 6 行，第 17 列，退出 1，数据库不存在 | 容器修改不能绕过源码翻译门禁 |

完整回归中的新场景 `missing_explicit_key_rejects_check_build_dev_and_run_without_touching_database` 对四个命令分别验证：缺词时失败、提供键名与源码位置，既有译文和迁移清单不变；空库不创建数据库，已有文件字节不变，不进入客户端工具链。另一个场景补齐全部语言后恢复 check。

### 浏览器与视觉

首次 ego 空白验收空间在后续调用时消失，工具报告 `task space not found`；已报告工具问题，未接管用户空间。经用户明确选择，改用 omp 默认浏览器能力，未覆盖配置、未另指定浏览器或用户资料目录。

在自有临时项目的真实页面中，用原生点击操作验证：

- `setLocale('cn')` 后 `useLocale()` 与 html.lang 均为 `zh-CN`，标题为“启动预检通过”，数量为“2 人”。
- `setLocale('en')` 后均为 `en-US`，标题为“Startup validation passed”，数量为“2 people”。
- React 草稿“语言切换保留的 React 草稿”与原生表单草稿“保留的报名草稿”保持；计数仍为 2。
- React 条件分支随语言变化，map 生成的 alpha/beta 列表正常；两次 `performance.timeOrigin` 都为 `1793893800514.6`，没有页面重载。
- 最终浏览器运行时错误集合为空；已检查整页截图。烟测夹具曾漏解构 setLocale，已修正夹具并重新加载后完成上述最终场景，不把该夹具错误归为产品错误。

本地证据保留在忽略目录 `artifacts/i18n-react-template-smoke/`：前后 CLI JSON、恢复结果、`browser-evidence.json` 与 `browser-english.webp`。不将个人临时路径或构建 Binary 入库。验收标签页、自有服务、临时项目、隔离 Binary 和一次性驱动脚本已清理；停止服务未冒充真实 Ctrl+C 验证。

### 已知限制与后续验证建议

- 本轮为 Windows 原生环境；未新增 Linux/macOS 行为结论，未执行发布工作流。既有 ignored 浏览器/显式用户依赖套件仍未执行，本次浏览器证明仅覆盖上述实际场景。
- 源码检查只覆盖 KEngine 已分析的项目模块，不实现全 TypeScript 类型检查或任意高阶函数、对象变更的数据流证明；不能证明的容器查表键或翻译能力透传会明确拒绝。动态业务数据不是可自动补译的词条。
- 默认入口是静态声明模板，不是执行任意 React 代码生成数据库 Schema；Hooks 与动态 UI 保持普通组件边界。
- 本轮 rust-analyzer 启动即退出，LSP 引用查询不可用；影响范围依据 KEngine/SWC 结构、模块引用及实际跨层回归确认。
- 后续发布前按新的版本安排更新兼容说明，并在目标平台重跑本记录的实际场景；不要将 dev 新能力误称为已发布的 0.4.4 能力。
