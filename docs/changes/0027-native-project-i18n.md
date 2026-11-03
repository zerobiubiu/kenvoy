# 0027：kenvoy 项目原生国际化与 CLI 0.3.0

- 状态：已执行；治理：不可变变更记录，定稿后以新记录追加变化。
- 日期：2026-11-03。
- 生效版本：Kenvoy CLI 0.3.0；KEngine 保持 0.2.0。
- 影响范围：项目配置、业务词典、CLI、产物、HTTP 描述与错误、原生 React Runtime、SDK、验证及文档。
- 前置记录：[0026：应用兼容与正式迁移](0026-application-migration-lifecycle.md)。本次产物 v3 替代其 v2，不重写原始实施事实。
- 发布边界：仅 dev 本地交付；未合并 main、未推送或发布，不声称 0.3.0 已可通过线上安装获取。

## 背景

用户要求 Kenvoy 原生处理 kenvoy 项目国际化，不让应用作者再选择翻译库、搭 Provider、复制系统文案或自行协调语言状态；本次明确要求递增版本。国际化只影响显示，必须与真实数据、权限、正式迁移和构建身份分别管理。

## 方案与公共契约

1. 增加可选 `[i18n]`，声明默认语言、支持列表和词典目录。create 生成原生中英文配置及业务词典；旧未启用配置继续单中文，不扫描偶然同名目录自动接入。
2. DSL 原有字符串是默认文案，资源/字段/静态选项/页面自动生成稳定词条键；普通展示文本用 `text:<原文>`。不执行翻译函数求 Schema，不翻译标识、业务记录、默认值或条件。
3. 框架自动提供语言上下文、可键盘操作的原生选择器、lang/dir/title、命名插值、按词典语言取复数和缓存 Intl 格式器。自定义组件直接使用 `@kenvoy/client` 的 `useI18n()`，不安装新国际化依赖。
4. 中英文系统界面与机器错误词典只有一份框架 JSON 来源，同时供前端与 CLI 提取消费。字段/API 错误新增 message_key/params，保留 code、默认 message、HTTP 状态和请求关联标识；结果未知仍不可自动重试。
5. Application IR 保持格式 1，Schema 哈希仍为 v1；客户端描述升级为 2，产物升级为 3。私有 i18n.json、i18n_config_hash 和 v3 输入/构建哈希绑定交付内容，完整产物无需源码或词典目录即可运行。
6. CLI 支持项目格式 `>=0.2.0, <0.4.0`；显式 i18n 要求最低 CLI 与项目格式均不低于 0.3.0。Kenvoy CLI 从 0.2.1 递增至 0.3.0，锁文件、版本日志、配置契约和安装说明同步；KEngine 没有该业务职责，不升版。
7. `i18n extract/check` 只读提取默认文案、检查词典及报告缺失，不覆盖翻译文件或访问数据库。缺少单条允许回退，重复键、错误类型、缺整份非默认词典及危险目录明确失败；普通 check 也验证词典。

完整现状契约见[原生国际化](../contracts/i18n.md)，不在本记录重复全部键和 SDK 参数。

## 实施文件

| 范围 | 文件与职责 |
|---|---|
| 版本 | `crates/kenvoy/Cargo.toml`、`Cargo.lock`、[版本日志](CHANGELOG.md)；仅 CLI 0.3.0 |
| 领域 | `src/domain/i18n/` 词典协议、严格解码、语言和默认词条；`domain/validation/` 机器错误及精确安全整数参数 |
| 项目与创建 | `src/infrastructure/project/{config,compatibility,create,mod}.rs`、`project/i18n/`；配置、路径、旧行为和双语模板 |
| 交付 | `src/infrastructure/artifacts/` 的格式、清单、私有文件、校验与配置摘要；`frontend/i18n.rs` 共享系统词典；`client_build/sources.rs` 内嵌新框架源码 |
| 生命周期与接口 | `application/publication/`、`bootstrap/{commands,lifecycle,server,i18n}.rs`、`interfaces/cli/`、`interfaces/http/{mod,wiring,projection,handlers,errors,i18n}.rs`；词典同代发布、授权投影和只读命令 |
| 原生前端 | `frontend/src/domain/i18n/`、`presentation/i18n.tsx`、`app/App.tsx`、`sdk.ts` 与表单/后台/记录/错误/上传展示；原有提交控制器与规范值保持同一入口 |
| 开发构建 | `frontend/scripts/client-dev.mjs` 仅放行固定四份系统词典 JSON；`frontend/tsconfig.json` 开启 JSON 模块类型支持；不开放任意用户 JSON |
| 回归 | `crates/kenvoy/tests/i18n_cli/`、`crates/kenvoy-tests/tests/native_i18n.rs` 与 `i18n_support/`、`frontend/tests/i18n*.test.mjs`；现有 HTTP/错误构造调用者及浏览器夹具同步 |
| 工程门禁 | `xtask/src/checks/architecture/{layout,typescript,tests}.rs` 区分 JSON 数据资产与代码，同时保留目标路径分层及缺失依赖诊断 |
| 用户追加示例 | `examples/i18n-app/`：完整配置、原生 DSL、SDK 组件、双语词典、README/ignore 及由真实 CLI 固化的 `000001` 迁移；本记录合并该追加交付 |
| 文档 | README、CLI 包 README、PRODUCT/DESIGN、配置/客户端描述/SDK/国际化、运行时/测试策略、渠道当前版本及各级索引 |

路径以 `crates/kenvoy/` 为源码前缀，跨 Crate 测试另行标明。未增加 Workspace 成员，未引入第三方 i18n 库，未操作用户业务库。

## 实际发现与修复

- 产物精确文件白名单最初没有 i18n.json，导致 8 个真实产物用例失败；将私有交付文件纳入原有白名单，没有放宽静态根。
- 配置目录落入 target/.env* 会被指纹/监听排除；配置路径和解析后目标均明确拒绝该类路径，普通 check 同时校验实际词典。
- Intl 日期样式最初与默认年月日参数冲突、精细 min/max 帮助被四舍五入、默认英文单数回退被当前中文复数类别误选；定向回归先出现 3 个失败，修复后全部通过。记录 ID 不跟随数字格式化改写。
- 安全整数边界曾以浮点参数序列化，JSON 往返可能损失末位；改用 i64 机器参数并验证往返，未放宽业务数值范围。
- 自定义 SDK 的生产打包成功不代表 dev 可用。真实浏览器首次白屏，资源记录证明内置 ui JSON 的 Vite 模块请求 404；开发代理只追加四份固定内置词典放行，重建同一隔离应用后页面正常，资源请求无 4xx。
- 验证期间修正 TS 可选字段收窄、未启用 reqwest JSON feature 的测试调用、无效 DSL 夹具和产物移动后的测试引用；不新增不必要依赖，不放宽正式语法或版本门禁，也不重钉偶然的 CLI 默认版本断言。
- 工程门禁最初把四份显式 JSON 导入判为依赖未解析。新增真实临时目录回归先失败，再将 JSON 资产路径独立收集并参与原有分层检查，不解析成 TS、不计代码行数；缺失文件和 domain→presentation 越界仍拒绝，没有新增忽略名单。

## 执行验证记录

### 环境与所有权

Windows 11 x64（10.0.26300），Rust Workspace 1.98/edition 2024，既有 Node/npm 与锁定框架工具链，Ego Browser 2.2.0（ego lite 0.6.2.7）。没有新增项目 i18n 库、依赖声明、全局工具或更新浏览器/设计技能；CLI 按既有行为在自有临时副本准备锁定前端工具链缓存。实际浏览器使用 omp 既有配置，仅为功能验收和用户追加示例各创建对应目标的 TaskSpace/唯一临时应用，使用不同回环地址，不复用已结束工作区或覆盖其他本地应用 Cookie。

### 已执行命令与结果

| 命令/场景 | 实际输出或观察 | 结论 |
|---|---|---|
| `npm run check --prefix crates/kenvoy/frontend` | 修复 Admin 可选 kind 收窄后退出 0 | TypeScript 通过 |
| `npm test --prefix crates/kenvoy/frontend` | 135 passed，0 failed；新增边界回归修复前 3 failed | 前端纯规则和错误协议通过 |
| `npm run build --prefix crates/kenvoy/frontend` | 129 modules transformed，真实 CSS/JS 资产生成，退出 0 | 内嵌生产 Runtime 可打包 |
| `cargo check --workspace --all-targets --offline --locked` | 退出 0 | 全目标类型检查通过 |
| `KENVOY_BINARY=.../target/debug/kenvoy.exe cargo test --workspace --offline --locked --no-fail-fast` | 退出 0；Kenvoy 单元 187 passed/1 ignored，i18n CLI 7 passed、原生跨层 6 passed、迁移契约 13 passed | 普通 Workspace 套件通过，不累计子进程重复输出 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 首次仅测试结构初始化 lint；修复后及收尾全目标均退出 0 | 无 allow 掩盖告警 |
| `cargo fmt --all`、`cargo fmt --all -- --check` 与 `cargo build --package kenvoy --offline --locked` | 均退出 0 | 格式和修复后的真实 CLI 已构建 |
| `kenvoy --version` | `kenvoy 0.3.0` | 用户要求的版本号已生效 |
| 真实 create/check/generate/seal/build/admin init | 自有项目形成两步正式链；build 前后不存在业务库，账号初始化后持有真实 SQLite | i18n 交付不把构建变为数据库操作 |
| 自定义 SDK 浏览器 | 无项目 i18n 库/Provider，真实 useI18n 组件显示长日期、带时区时间、英文回退 `1 item`，插值标签保持文本且无 img 节点 | SDK、复数、格式样式和转义实际运行 |
| 中英文切换 | lang/title/界面同步；文档标记仍在，表单草稿和 SDK 计数 1 保留；Session/application 请求数仍为 2，generation=1 | 不重载、不刷新身份或代际 |
| 精确数字与规范写入 | 错误最小值及帮助保留 0.0001/1.2345；真实保存中文原文、数值 0.0001、选项 pending、布尔 false | 显示语言不改业务值 |
| 日历日期与迁移隔离 | 真实键盘编辑日期、切换语言后保留并提交；SQLite 第二条记录为 `2026-01-02`，Schema/hash 不变，正式迁移计数始终为 2 | 日期草稿不是格式化后的展示字符串，语言不产生迁移 |
| 阿拉伯语/窄屏 | 键盘选择语言后 lang=ar、dir=rtl，390×844 文档宽度仍为 390，无页面横向溢出，SDK 计数仍为 1；导航后偏好仍为 ar | 验证部分阿语业务词典、系统英文回退与状态持久化 |
| 真实后台/错误 | 错误登录后同一核对编号的英文错误切换为中文；用户名保留；正确登录后两次描述角色均为 admin；后台显示 0.0001、Pending、No 及本地化日历日期 | 错误不解析中文，切换不改变权限或存储值 |
| 最终定向跨层 | `cargo test --package kenvoy-tests --test native_i18n --offline --locked`，显式 KENVOY_BINARY | 修复后真实二进制 6 passed，0 failed |
| `cargo test --package xtask --offline --locked`、`cargo xtask check`、最终 `cargo xtask docs` | 31 passed；追加示例后四成员及依赖方向通过，422 自有代码文件、最大 449/500 行；最终 99 篇 Markdown、1024 本地链接通过 | JSON 资产分层回归及工程/文档门禁通过 |
| `impeccable detect --json crates/kenvoy/frontend/src crates/kenvoy/frontend/index.html` | 退出 0，原始结果 `[]` | 静态检测无命中，不替代浏览器证据 |
| 开发私有边界 | 内置 ui JSON 的 Vite 模块 200；真实存在的框架 package.json、交付 i18n.json 和源词典 URL 均 404 | 修复没有扩展为任意 JSON/私有文件公开 |
| 用户追加 i18n-app | 真实 check、i18n check、generate/seal 退出 0；zh-CN/en-US 原生键缺失均为空，生成安全初始步骤 000001 | 可直接运行，不是代码片段或未固化脚手架 |
| 示例打包与无源码部署 | 临时副本 build 退出 0，前后无数据库；移出 src/locales/migrations/.kenvoy 后只用配置和 dist 执行 run，空库执行完整一步链 | 自定义 SDK 与词典均不依赖部署时源码或前端工具链缓存 |
| 示例真实浏览器/SQLite | `Demo count: 2 people` 与草稿跨语言保持；提交按钮覆盖正常；保存原文、guests=2、session=afternoon、newsletter=false，迁移计数 1；匿名列表 401 | 示例包含实际国际化、持久化与权限闭环，仓库示例未生成业务库 |

全量 Rust 原始日志保留在本机忽略目录 `artifacts/native-i18n/rust-acceptance.log`；不是随产品发布的业务数据。忽略的七套既有浏览器与自定义依赖构建入口不计为本轮普通 Rust 通过项。

### 限制与后续验证建议

- 内置完整系统语言为中文和英文；其他语言需维护项目词典并按规则回退，不宣称自动翻译业务内容、记录或浏览器自身的日期弹层。
- 未运行跨平台六目标发布/渠道上传或其他操作系统浏览器，未使用真实业务数据库；0.3.0 仍待用户决定发布。
- Windows Cargo 曾输出 incremental finalize 的 os error 5 非致命提示，最终测试退出 0；没有修改 ACL、关闭安全软件或持久禁用增量编译。
- 本机 LSP 引用查询时 rust-analyzer 退出、设备无可用语言服务，已报告；使用定向代码引用与编译/真实调用替代，不宣称 LSP 成功。
- 浏览器工具存在截图限制：常规截图多次报告最小化或超时；原始 `fromSurface:false` 返回的不是目标应用，已立即废弃并删除、报告宿主问题，不能当作视觉证据。实际语言、状态、错误、规范写入与 390px 几何通过真实 DOM/交互/SQLite 验证，不据此宣称完成可靠像素级复核。
- 日期自动填充曾只改变 DOM 而未形成 React 草稿，未将该值当作保存成功证据；改用真实键盘编辑、语言重渲染后保留及 SQLite 原值核验。没有在产品中加入绕过受控字段的修补。
- Windows PTY 监督器在真实 Ctrl+C 后报告过退出码 512；已从应用日志观察到 stopped，且独立连接确认功能验收端口关闭，不将该宿主返回值描述为应用退出 0。
- 后续扩展语言时补全所需系统/业务词典并复用提取报告；增加浏览器平台或地区时分别核验数字、日历/时区及原生控件，不把本机语言切换证据扩大为全部地区验证。
