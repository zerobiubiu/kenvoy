# 0028：强制原生语言、集中译文与 JSON Schema

- 状态：已执行；治理：不可变变更记录，定稿后以新记录追加变化。
- 日期：2026-11-03。
- 生效版本：Kenvoy CLI 0.4.0；KEngine 保持 0.2.0。
- 影响范围：语言配置、CLI 初始化/启动、文件契约、产物、HTTP/SDK、设备语言、五个示例及回归。
- 替代关系：[0027](0027-native-project-i18n.md) 的可选 i18n、分语言文件和提取命令被本次替代；旧记录不重写。正式迁移沿用 [0026](0026-application-migration-lifecycle.md)，已 sealed 迁移不改写。
- 发布边界：dev 本地交付，未合并 main、未推送或发布；本地版本号和标签不代表线上可安装。

## 背景与用户确认

用户明确国际化是原生必需能力，不能关闭；语言代码、可选别名和显示名统一在 kenvoy.toml 的 languages 中声明，业务翻译集中于根 translations.json。设备默认语言受支持时使用该语言，否则 fallback，不能沿用旧版本优先持久化选择的策略。

实施前确认了两项未定口径：顶层只有 `$schema` 必填，title/description/content 是示例而非固定业务字段；首次 `kenvoy i18n` 仅创建 Schema 引用，不复制默认文字冒充其他语言的翻译。已有任意业务词条则必须提供全部配置语言的非空值。

## 契约与实现方案

1. Config.languages 不再是 Option，国际化始终启用。完全省略表使用中文/英文/韩文默认定义；显式表只包含实际声明项，不隐式补语言。fallback 必须精确等于代码；display 必填，alias 只能一个、可缺但非空，代码/别名共享不区分大小写的唯一命名空间。
2. translations.json 顶层声明 `"$schema":"./language.schema.json"`，业务键映射完整语言对象；同一语言恰好选代码或别名一个。未知键、漏语言、重复/转义重复 JSON 键、错误类型和空译文拒绝。
3. 生成 draft 2020-12 Schema，为顶层 `$schema` 设置独立 properties/const/required，避免原示例的 additionalProperties 误判元数据；业务对象用 allOf 和 oneOf 确保语言完整且唯一。根 minProperties=1，metadata-only 文件合法。
4. 固定引入 `jsonschema =0.33.0, default-features=false`，以真实验证器校验生成契约，不使用简易正则代替 Schema，不启用外部 HTTP/文件引用解析。选择该版本避免新验证器传递启用 serde_json/float_roundtrip、改变无关业务浮点解析。解析器先拒绝重复键，验证通过后将别名归一到代码。
5. `kenvoy i18n` 不读取源码或数据库：先检查配置，原子刷新 Schema（相同字节不重写），仅缺失时 create_new 译文，然后校验。已有文件无论有效与否都不覆盖。固定根文件拒绝链接/目录，读取上限 32 MiB；只清理本次独占的临时输出。
6. dev/run 在应用代码、数据库创建/迁移、服务之前执行相同预检，但不创建译文。缺文件提示 i18n 或手工带 Schema 创建；已有坏文件只保留现场，不静默忽略。dev 配置变化也刷新 Schema并检查，坏候选保留活动代际。
7. 每次打开/刷新读取 navigator.language，代码精确匹配（忽略大小写）或 fallback；不猜测基础语言，不访问其他偏好或旧 localStorage。原生选项使用 display，页内 setLocale 支持代码/别名并规范化，保留表单/上传/Hooks 状态。
8. CLI 升为 0.4.0，支持项目格式 `>=0.4.0, <0.5.0`，最低 CLI 不低于 0.4.0；Artifact 4、HTTP 描述 3、Bundle 2、input/build hash v4。Application IR 1、Schema hash v1、迁移状态不变。
9. 配置摘要为 languages_config_hash；输入包含 translations.json，派生 language.schema.json 和临时写入不参与指纹/监听，避免自激构建。无源码也必须携带合法且与打包 Bundle 相同的根译文，不能用旧产物绕过新契约。

完整有效规则及升级方式见[语言契约](../contracts/i18n.md)、[配置](../contracts/project-configuration.md)、[CLI](../operations/cli-design.md)，不在本记录维护另一份完整字段手册。

## 实施文件与删除范围

| 范围 | 主要文件/职责 |
|---|---|
| 版本与依赖 | 根/CLI Cargo.toml、Cargo.lock：CLI 0.4.0、jsonschema 精确依赖，KEngine 不变 |
| 领域 | `crates/kenvoy/src/domain/i18n/{config,mod,decode,tests}.rs`：强制配置、语言身份、规范化矩阵；删除无生产调用的默认词条提取 collector |
| 项目与文件 | `infrastructure/project/{config,compatibility,create,mod}.rs`、`project/i18n/{schema,files,messages,mod,tests,schema_tests}.rs`：Schema、严格 JSON、原子更新/文件保护、三语言模板 |
| CLI/生命周期 | `interfaces/cli/{args,mod}.rs`、`bootstrap/{commands,i18n,lifecycle}.rs`：唯一 i18n 命令、启动前预检与热更新 |
| 交付/投影 | `infrastructure/artifacts/{mod,staging,load,fingerprint,catalog_tests}.rs`、`interfaces/http/{projection,i18n}.rs`：版本、配置摘要、根/打包词典一致、授权投影 |
| 前端 | `frontend/src/domain/i18n/{model,locale,translate}.ts`、`presentation/i18n.tsx`、App/类型/API 与 i18n 测试：设备选择、display、Bundle2/HTTP3、移除旧存储和 LEGACY |
| 清理 | 删除旧 I18nConfig/可选协议、extract/check 子命令和仅供提取的 Rust 系统词典模块，不保留兼容 shim |
| 回归 | CLI i18n_cli、跨层 native_i18n/i18n_support、全部受影响临时配置与浏览器脚本；新增显式 `browser::i18n` 场景 |
| 五个示例 | customer/extensions/options/uploads/i18n-app 的 0.4 配置、根 Schema/译文、README；i18n-app 27 个词条完整矩阵混用代码/别名，删除过时 locales 两文件；原始迁移保持 |
| 文档 | README/CLI README、PRODUCT/DESIGN、语言/配置/客户端描述、运行时/CLI/测试/依赖与索引；不改写 0027 正文 |

未标前缀的产品源码路径位于 `crates/kenvoy/src/`。没有新增 Workspace 成员、没有把语言/数据库业务放入 KEngine，也没有在示例业务库试验。

## 实际发现与修复

- 首次并行执行前端 Vite 构建与 Rust check 时，Vite 删除旧摘要资产，Rust 已生成的 include_bytes 清单仍引用旧文件。改为前端完成后再编译；这是验证依赖顺序问题，没有通过保留陈旧资产绕过。
- 清除只供测试使用的 LanguageDefinition 门面重导出，测试改为真实内部类型入口；不以 allow 隐藏 unused import。
- Windows mtime 回归最初对只读 File 句柄调用 set_times，产生 os error 5；修正测试为写属性所需的写句柄，保留固定历史时间及幂等断言，没有修改产品文件保护。
- Schema 默认错误会输出整条译文及英文上游说明；改为中文规则说明、实例 JSON Pointer 和 Schema 位置，不回显译文值。真实双键失败已重放，新增数据不进诊断的回归。
- 严格 Clippy 检出两处测试 Default 后字段赋值，改为结构初始化器；没有弱化规则或删除有效边界测试。
- 最终依赖树检查发现候选 jsonschema 0.60.2 即使禁用默认 feature，仍强制合并 serde_json/float_roundtrip。未将该无关数值行为变化交付：改用支持相同 draft 2020-12 的 0.33.0，保留 serde_json 原 default/raw_value/std，重新执行完整验证。未更改 Schema 哈希协议或给 KEngine 增加业务依赖。
- 最终全量复验暴露原 DSL 安全测试夹具的并行目录名碰撞（同进程/时钟粒度，os error 183）。追加单调原子序号保证独占目录，不吞掉 create_dir 错误、不减测；随后同一完整 Workspace 以 no-fail-fast 复验全部通过。

## 执行验证记录

### 环境与所有权

Windows 11 x64（10.0.26300）；实际 `rustc/cargo 1.99.1`、Node `24.22.0`、npm `12.2.0`，Workspace 声明 MSRV 1.98。Ego Browser 2.2.0 使用 omp 既有配置，本目标独占 TaskSpace 和临时应用，loopback `127.77.85.65` 避免碰其他应用 Cookie。新增 Cargo 依赖来自 crates.io 固定版本/校验值，关闭默认引用解析；没有安装项目翻译库或更改浏览器配置。

### 命令与实际结果

| 命令/场景 | 实际结果 | 结论 |
|---|---|---|
| 验证器元数据、manifest、`cargo fetch` 与 `cargo tree --offline --locked -e features -i serde_json --depth 1` | 候选 0.60.2 的强制 float_roundtrip 被发现；最终固定 0.33.0，serde_json 仅 default/raw_value/std | 使用标准验证器且不连带改变业务数值解析 |
| `npm run check`、`npm test`、`npm run build --prefix crates/kenvoy/frontend` | 类型检查 0；143 passed/0 failed；129 modules transformed，生产资产生成 | 前端契约与构建通过 |
| `cargo check --workspace --all-targets --offline --locked`、`cargo build --package kenvoy --offline --locked` | 在前端资产完成后退出 0 | 新协议真实 CLI 可构建 |
| `cargo test --package kenvoy --bin kenvoy --offline --locked infrastructure::project::i18n` | 15 passed，0 failed | Windows mtime 修复、Schema/文件边界及诊断通过 |
| `cargo xtask test`；最终固定验证器后显式 `KENVOY_BINARY` 执行 `cargo test --workspace --offline --locked --no-fail-fast` | 最终退出 0；Kenvoy 单元 195 passed/1 ignored，i18n CLI 14 passed、native_i18n 10 passed、迁移契约 13 passed、xtask 31 passed | 完整普通 Workspace 通过，不叠加子进程重复行 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`、`cargo fmt --all -- --check` | 修复测试初始化器后均退出 0 | 严格质量门禁通过 |
| `cargo xtask check` | 四成员与依赖方向通过；427 自有源码、最大 449/500 行；100 篇 Markdown、1038 个本地链接通过 | 架构、规模与文档门禁通过 |
| `kenvoy --version` | `kenvoy 0.4.0` | 包版本与真实二进制一致 |
| 无源码/无数据库的四语言 CLI 冒烟 | 首次 i18n 仅创建 Schema 引用；第二次 schema_updated=false、translations_created=false 且 mtime 不变 | 初始化不编译应用、不写数据库、不覆盖文件 |
| 用户原样四语言样例 | title/description/content 混用 zh-CN/cn、en-US/en、ko-KR、ja-JP，命令退出 0、message_count=3、译文字节不变 | 指定 Schema 语义真实可用 |
| 八类非法译文 | 缺/错 Schema 引用、代码+别名双写、漏语言、空值、未知语言、重复键及转义重复键均退出 1，文件字节不变 | 非法内容拒绝，不通过自动补齐掩盖问题 |
| dev/run 非法输入冒烟 | 都在根译文检查时报错，未创建业务库；完整跨层还验证缺文件、待迁移库、无源码恢复 | 启动前门禁和数据保护通过 |
| 五例独占副本 `i18n` 与 `check` | 全部退出 0；Schema/译文字节均不变，未创建任何业务库；i18n-app 27 词条 | 正式示例与实际生成规则一致 |
| 真实浏览器初始设备 | navigator.language=zh-CN，项目 fallback=en-US，实际选 zh-CN；预设旧存储 en-US 后刷新仍选 zh-CN | 设备优先且不受旧选择覆盖 |
| 配置 display 与 SDK alias | 选项显示“简体中文（配置）/English (configured)”；SDK `en` 切至 en-US，计数 2 和草稿保留，Session/application 请求数不增 | 显示名、别名和状态保持真实运行 |
| 不支持语言及其他偏好 | CDP 模拟 zh-TW/第二偏好 zh-CN，以及设备 cn/第二偏好 zh-CN，均选择 fallback=en-US | 不猜基础语言、不把设备别名或第二偏好当支持代码 |
| 不可读设备设置 | 在自有页面注入抛错的 navigator.language getter，实际界面使用 en-US fallback | 覆盖读取异常；明确属于浏览器模拟，不冒充物理设备 |
| 刷新及写入 | 刷新重新选设备中文、计数回初始；真实表单保存 name 原文、guests=3、session=afternoon、newsletter=false，迁移仍一步 | 显示不改变规范值或迁移 |
| HTTP 投影 | description=3、bundle=2、仅代码消息映射、role=anonymous，匿名读取列表 401 | 新契约不提升权限 |
| 视觉 | 实际检查 1280×900 桌面及 390×844 图片；移动文档宽 390，无横向溢出，选项文字完整 | 本轮取得了可靠目标截图，不沿用 0027 的截图限制结论 |
| `impeccable detect --json crates/kenvoy/frontend/src crates/kenvoy/frontend/index.html` | 退出 0，结果 `[]` | 机械检测无命中，不替代实际交互 |
| 无源码额外冒烟 | 移出 src/migrations/.kenvoy 后，缺根译文的 run 返回 1 并重建 Schema；恢复原译文后 run 就绪，浏览器显示 SDK 译文，development/根词典/Schema URL 均为 404 | 无源码也检查集中译文，私有文件不公开 |
| 最终固定验证器产物复验 | 使用最终二进制重建独占副本后移出源码/工具链，run 与浏览器再次成功；规范记录和迁移审计与重建前逐项相同 | 最终依赖版本的真实运行与数据保护通过 |

原始日志、CLI 样例与数据核验位于本机忽略目录 `artifacts/languages-contract/`；最终全量日志为 `workspace-tests-final.log`，截图为 `desktop-configured-display.png`、`mobile-configured-display.png`。不将测试临时数据库、构建缓存或浏览器脚本纳入产品提交。

### 未覆盖范围与后续建议

- 本轮没有执行八套独立 Playwright 浏览器套件或忽略的第三方客户端构建用例；使用 omp 提供的真实浏览器验收本次路径，普通 cargo test 的 ignored 不计为通过。
- 设备语言异常/变体使用本目标页面的 CDP/属性模拟，未声称覆盖实体 iOS/Android 或其他浏览器；没有重新运行 MSRV 1.98 或六平台发布矩阵。
- 内置完整系统语言仍为中文/英文；其他语言需集中词条覆盖，缺整个业务键使用已有 DSL/调用方默认文本。metadata-only 是用户明确认可的合法起点，不是假翻译。
- Windows 原子刷新及重启路径已实际使用；Unix 链接测试在本机 cfg 排除，磁盘耗尽、断电和并发恶意文件替换未扩展为已验证能力。
- 最终 Cargo 输出过 incremental finalize 的 os error 5 非致命提示，全套退出码仍为 0；未修改 ACL、安全软件或持久关闭增量编译。PTY 监督器在 Ctrl+C 后可能报告 512，应用停服与端口释放另行观察，不将监督器值冒充应用成功退出码。
- 当前可写任务保留根 .gitignore 中用户已有的 .markindex/ 改动，不纳入此逻辑提交；已发布标签不移动，不补发历史版本。
