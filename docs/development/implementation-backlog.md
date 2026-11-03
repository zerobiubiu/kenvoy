# 后续实施与验证待办

状态：参考；治理：现状待办；核对日期：2026-11-02。

本页保存符合性核查发现的实现完善项、待补验收证据和可选扩展，供后续逐项实施。初始来源为源码、既有测试与历史日志，以及 CLI 只读分析；后续已执行项分别链接真实实施和验证记录。尚未执行的条目均未排期，不代表已授权立即改代码、安装依赖、迁移数据或发布。

返回[开发文档入口](README.md)或[文档中心](../README.md)。完成范围见[阶段路线](development-roadmap.md)，风险控制与环境限制见[技术风险](technical-risks.md)。

## 已确认设计，不属于待补缺陷

- Workspace 保留 `kengine`、`kenvoy` 两个产品成员，以及不发布的 `kenvoy-tests`、`xtask` 工程成员；产品依赖仍为 `kenvoy → kengine`。这是用户明确设计，依据见 [ADR 0011](../decisions/0011-rust-test-workspace-and-xtask.md)，不安排恢复为严格两个 Workspace 成员的任务。
- 默认前端保留 React、原生控件与 CSS；允许用户选择兼容的 React 组件库。这是用户明确设计，依据见 [ADR 0010](../decisions/0010-native-components-and-data-bindings.md)，不安排恢复默认 MUI/Emotion 依赖的任务。
- 文档按类型子目录归档是当前文档规范，不因早期提示词中的平铺示意而迁回旧路径。

## 使用方式与状态

- `I`：已有能力的实现完善项；I05 的显式字段规则已执行，其余能力与组合验证边界按条目单独说明。
- `V`：现有能力的验证补齐项；V01 已随本轮附件验收补齐，其他条目仍待执行。缺少直接证据不等于功能不存在，也不能把历史通过写成本轮复验。
- `E`：可选扩展或条件性兼容验证；明确需求与方案后再实施，不追溯为首版全部失败。E04 及 E06 的发行部分已由用户明确纳入并实施，其余候选范围未因此获得实施授权。
- 条目编号只用于本页追踪，不表示优先级、发布版本或执行顺序。实施前重新核对代码与依赖关系，不把本页的方案方向直接当作已定接口。

## I：实现完善

### I01：细粒度增量分析与跨模块链接

- **当前边界**：`AnalysisSession` 真实缓存未变化模块的解析结果及模块内语义，但每轮仍读取所有可达源码，重新定位依赖、建立模块图、分析循环并执行跨模块链接；`affected` 尚未用于约束整条分析管线。
- **后续工作**：明确受影响集合与缓存失效协议，减少不必要的重读、整图重算和 IR 复制；保留新增、删除、重命名、导入目标及解析选项变化时的正确性。避免为了缓存命中而遗漏文件系统变化。
- **验收标准**：连续编辑序列的结果与全量分析一致；未变模块不重解析；受影响分析范围可解释；错误候选不替换最后有效结果；报告实际解析、链接、耗时和内存变化。
- **相关入口**：[增量会话](../../crates/kengine/src/application/incremental.rs)、[分析管线](../../crates/kengine/src/interfaces/engine.rs)、[增量回归](../../crates/kengine/tests/incremental.rs)、[KEngine API](../contracts/kengine-api.md)。

### I02：独立模块并行处理与大项目性能

- **当前边界**：正式分析按文件同步遍历；没有并行解析或大项目吞吐、延迟、内存占用的验收数据。会话内缓存不等于跨进程持久化缓存。
- **后续工作**：先建立小、中、大多模块项目的可复现基线，再评估独立模块并行、缓存数据共享和内存预算；持久化缓存只有在收益及失效协议明确后再决定是否引入。
- **验收标准**：并行与串行结果、诊断顺序及源码位置保持确定；错误、取消和资源回收正确；记录机器、项目规模、冷热运行、并发度、耗时与峰值内存，不以解析次数代替性能数据。
- **相关入口**：[分析管线](../../crates/kengine/src/interfaces/engine.rs)、[增量会话](../../crates/kengine/src/application/incremental.rs)、[模块与符号测试](../../crates/kengine/tests/)、[技术栈](../reference/technology-stack.md)。

### I03：跨错误类别的统一诊断与关联位置

- **当前边界**：Language、DSL、JavaScript 编译诊断已有错误码、主位置与建议；普通配置、I/O、数据库、迁移、产物和构建错误未统一具备诊断字段。部分 DSL `related` 位置及展开链已保存在内部，但没有完整输出。
- **后续工作**：统一适用的错误类别、代码、原因、建议和来源链，补齐终端及结构化日志中的关联位置；无源码位置的错误不得虚构行列。
- **验收标准**：覆盖源码与非源码错误的代表性失败；重复声明及 Schema 展开错误保留真实关联位置；文本与 JSON 输出语义一致；错误链可定位，同时不输出密码、Cookie、Token 或业务数据。
- **相关入口**：[错误与诊断输出](../../crates/kenvoy/src/bootstrap/error.rs)、[CLI 失败分支](../../crates/kenvoy/src/main.rs)、[DSL 诊断](../../crates/kenvoy/src/application/compilation/mod.rs)、[CLI 测试](../../crates/kenvoy/tests/cli.rs)。

### I04：服务端异常上下文与可定位执行日志

- **当前边界**：QuickJS 多数运行异常被泛化为 `Execution`；Action 错误日志不保留原异常上下文。现有执行统计主要是 DEBUG 级别的 handler、耗时和成功标志，请求标识没有贯穿相关运行任务。部分 watcher 错误也没有保留原始原因。
- **后续工作**：区分对客户端脱敏与在服务端保留安全诊断上下文，补充可定位的错误类别、登记函数或模块信息、适用的异常来源及请求关联；为 watcher 故障保留原因。日志字段与级别需要明确设计，不直接记录函数参数、返回业务值或敏感内容。
- **验收标准**：可信函数抛错、模块加载失败、数据库失败和 watcher 错误可在服务端定位；客户端响应仍不泄漏内部堆栈、SQL、路径或密钥；失败后下一次正常调用及资源释放不受影响。
- **相关入口**：[JavaScript Runtime](../../crates/kenvoy/src/infrastructure/javascript/runtime/mod.rs)、[Action 处理器](../../crates/kenvoy/src/interfaces/http/handlers.rs)、[HTTP 错误映射](../../crates/kenvoy/src/interfaces/http/mod.rs)、[开发生命周期](../../crates/kenvoy/src/bootstrap/lifecycle.rs)、[运行时契约](../architecture/javascript-runtime.md)。

### I05：字段改名检测与显式映射迁移

- **已执行范围**：正式迁移提供同表显式 `renames`、`backfills`、`conversions` 和风险/断言协议，映射不依赖相似名称猜测；字段改名、回填、改型、约束及附件 BLOB 保留/回滚已有定向验收，见 [0026](../changes/0026-application-migration-lifecycle.md)。
- **保留边界与后续验证**：表级保留数据改名未纳入本轮；多字段与复杂组合仍需补充独立数据矩阵，不把现有单字段组合用例扩大为所有组合已通过。表名变化不能把删旧建新包装为改名。
- **验收标准**：单字段、多字段、歧义映射、冲突和组合变化均有明确结果；成功后数据、索引及约束保留；失败不改变原库；继续要求 checksum 确认、数据预检、备份与事务保护。
- **相关入口**：[迁移 Diff 与执行](../../crates/kenvoy/src/infrastructure/persistence/sqlite/migration/mod.rs)、[迁移 CLI](../../crates/kenvoy/tests/migrations_cli.rs)、[迁移执行协议](../operations/migration-design.md)。

## V：验证补齐

### V01：同一项目的 dev → run 数据库交叉验收

- **状态**：已执行；本轮附件专项覆盖同一独占项目的 dev 写入、停服、不预先 build 直接 run、读取及编辑原记录。
- **执行证据**：同一配置路径 `data/app.sqlite` 中记录 1/3 和附件持续存在；生产重新读取 PNG/PDF，管理员通过浏览器编辑原记录并替换附件，SQLite 直接读回仍为原记录及正确文件数量。生产 development/HMR 两端点均为 404，目录未生成第二份数据库。
- **记录与边界**：实际命令、HTTP/浏览器/SQLite 结果和环境见[0014 变更记录](../changes/0014-file-uploads-and-images.md)；仅证明该 Windows 项目场景，不扩大为 V02/V05/V06 的退出、平台或故障矩阵通过。
- **相关入口**：[生产浏览器流程](../../crates/kenvoy/frontend/scripts/production-e2e.mjs)、[HTTP 与重启集成测试](../../crates/kenvoy-tests/tests/http_cli.rs)、[运行入口](../../crates/kenvoy/src/main.rs)。

### V02：当前修订的真实 Ctrl+C 与资源退出

- **当前证据**：较早阶段有独立 PTY 的 Ctrl+C 记录；2026-11-03 的当前构建已复核基础 `dev`、带 Vite 子进程的自定义 `dev` 和生产 `run` 的 Ctrl+C，观察到完整关闭日志、端口释放、Node/CLI 无残留及 `.kenvoy/client-dev` 清理，见 [0025](../changes/0025-graceful-shutdown.md)。尚未在写入数据后重启核对，也未连接浏览器开发 WebSocket；七套浏览器夹具仍使用强制终止。
- **待补验证**：在真实登记/编辑数据后停止并重启核对记录；通过浏览器保持开发 WebSocket 活跃再发送 Ctrl+C；在 Unix 发送 SIGTERM；覆盖构建中途取消、第二次信号与超时强制路径。
- **验收标准**：HTTP 停止接收、端口释放、WebSocket 关闭、watcher/Vite/JS/SQLite 等自有资源回收、无本项目残留子进程、重启后数据存在；区分 PTY 外层退出码与应用本身结果。
- **相关入口**：[进程夹具](../../crates/kenvoy-tests/src/lib.rs)、[退出观察脚本](../../crates/kenvoy/frontend/scripts/shutdown-observer.mjs)、[生命周期历史证据](../verification/0011-cli-lifecycle.md)、[退出实现](../../crates/kenvoy/src/main.rs)。

### V03：泛型 React 组件的真实浏览器场景

- **当前证据**：已有 TypeScript 泛型解析、转换和切片回归，但当前扩展浏览器场景没有专门使用泛型 React 组件。
- **待补验证**：增加跨文件泛型组件及 Props、Hooks、事件和 CSS 的真实构建/浏览器流程，不把编译去类型测试当作 DOM 行为证明。
- **验收标准**：组件实际呈现，Props 与交互返回正确，重复 React 实例未出现，服务端私有模块不进入客户端；不因此承诺框架已提供完整用户项目 TS 类型检查。
- **相关入口**：[扩展浏览器流程](../../crates/kenvoy/frontend/scripts/extensions-e2e.mjs)、[源码切片回归](../../crates/kenvoy/src/application/compilation/client_slice/mod.rs)、[客户端构建回归](../../crates/kenvoy/frontend/tests/client-builder.test.mjs)。

### V04：错误、空状态与查询结果的浏览器断言

- **当前证据**：加载、错误、空态、未知写入结果和组件错误边界有实现或部分客户端测试；真实浏览器没有完整状态矩阵。后台排序的浏览器断言主要检查 `aria-sort`，未直接核对行值顺序。
- **待补验证**：覆盖列表/启动失败、断连、超时、写入结果未知、空列表、无匹配、组件渲染异常，以及分页/排序后的实际内容；验证重试与用户操作不会造成静默重复写入。
- **验收标准**：真实界面状态、可恢复操作及请求/数据结果相互对应；排序断言检查实际返回和显示顺序；客户端替身测试与真实 HTTP/浏览器证据分别记录。
- **相关入口**：[通用 Form/Table Runtime](../../crates/kenvoy/frontend/src/presentation/runtime/RuntimePage.tsx)、[客户端 API](../../crates/kenvoy/frontend/src/infrastructure/api.ts)、[后台浏览器流程](../../crates/kenvoy/frontend/scripts/admin-e2e.mjs)、[API 单元测试](../../crates/kenvoy/frontend/tests/api.test.mjs)。

### V05：目标平台、浏览器与最低版本运行矩阵

- **当前证据**：主要实测环境是 Windows/Chromium；Rust 最低版本的 Workspace 编译通过不等于在该版本完成所有运行测试。其他系统、浏览器、实体移动设备与辅助技术矩阵未齐。
- **待补验证**：依据实际部署目标确定平台与浏览器集合，补最低 Rust 版本的运行测试、目标系统的进程/路径/关闭行为，以及需要支持的移动端和无障碍交互。
- **验收标准**：每个目标有独立环境、版本、命令、实际结果及限制；没有执行的目标明确留空，不以本机结果推定跨平台通过。
- **相关入口**：[测试策略](testing-strategy.md)、[测试夹具](../../crates/kenvoy-tests/src/lib.rs)、[技术风险](technical-risks.md)、[工程重组验证记录](../changes/0013-rust-test-workspace-and-tools-retirement.md)。

### V06：生命周期故障、压力与恢复边界

- **当前证据**：正常发布、失败候选保留、数据库回滚及部分取消/关闭路径有测试；提交后磁盘/代理失败、磁盘满、断电、网络盘、构建或发布中途退出、事件风暴及长时负载未全面验收。HTTP/JS 关闭没有统一进程级最终期限，不能仅凭静态代码断言所有异常退出始终有界，也不能据此认定已经发生挂死。
- **待补验证**：在隔离环境设计故障注入与压力流程，覆盖提交前后、watcher 失效、任务取消和资源关闭；若暴露问题，再明确修复与恢复协议。
- **验收标准**：提交前失败可保持旧状态；数据库已提交后不伪称回滚，必须能定位当前代际并给出恢复或重启路径；数据完整性、退出期限、自有资源回收及重启行为有实际证据，不能使用生产数据进行破坏性验证。
- **相关入口**：[开发生命周期](../../crates/kenvoy/src/bootstrap/lifecycle.rs)、[HTTP 发布](../../crates/kenvoy/src/interfaces/http/mod.rs)、[数据库线程](../../crates/kenvoy/src/infrastructure/persistence/sqlite/worker/mod.rs)、[JavaScript 关闭](../../crates/kenvoy/src/infrastructure/javascript/runtime/mod.rs)、[风险登记](technical-risks.md)。

## E：可选扩展与条件性验证

### E01：业务关系与物理外键 Schema

- **当前边界**：SQLite 外键开关和内部认证外键存在，但业务字段/Schema 没有关系目标、引用列及级联策略；`optionsSource` 是受控选项查询与写入校验，不是物理外键或自动级联。
- **启用条件与范围**：明确业务关联需求后，再设计 DSL、IR、引用/删除策略、Schema Diff、CRUD 权限及迁移；关系字段允许后续实现，不把当前未提供关系能力误写成基础持久化失效。
- **验收标准**：真实外键约束、关联权限、来源删除/修改、存量数据迁移、回滚与重启一致性均验证；仅启用 PRAGMA 或手写测试外键不能算 DSL 关系能力完成。
- **相关入口**：[字段模型](../../crates/kenvoy/src/domain/model/mod.rs)、[迁移模型](../../crates/kenvoy/src/infrastructure/persistence/sqlite/migration/mod.rs)、[选项规则](../contracts/component-bindings.md)。

### E02：受控资源、文件与网络 Host API

- **当前边界**：QuickJS 只执行可信项目的纯计算，未提供 `ctx.resources` 或文件/网络/数据库 Host API。
- **启用条件与范围**：按实际场景分别设计最小能力、权限、参数、错误转换、事务和生命周期边界，不一次性开放完整宿主或 Node/Bun API。
- **验收标准**：通过受控 API 执行真实允许操作，越权与非法输入被拒绝，取消/超时后资源正确回收；不暴露 SQLite 连接、任意 Rust 指针、进程或敏感环境。
- **相关入口**：[JavaScript 契约](../architecture/javascript-runtime.md)、[运行时实现](../../crates/kenvoy/src/infrastructure/javascript/runtime/mod.rs)、[HTTP Action](../../crates/kenvoy/src/interfaces/http/handlers.rs)。

### E03：不可信 JavaScript 执行隔离

- **当前边界**：`runtime.trust=untrusted` 明确拒绝；当前时间、内存与模块限制不是操作系统沙箱。
- **启用条件与范围**：只有出现不可信执行需求时，才设计独立进程或操作系统隔离、能力协议、限额、取消与强制回收；此前继续拒绝执行。
- **验收标准**：恶意死循环、微任务、OOM、非法宿主访问和进程故障不能突破权限或拖垮宿主；终止后无自有资源残留，后续正常请求可运行。
- **相关入口**：[可信边界](../architecture/javascript-runtime.md)、[服务端编译](../../crates/kenvoy/src/infrastructure/javascript/compiler/mod.rs)、[运行时测试](../../crates/kenvoy/src/infrastructure/javascript/runtime/tests.rs)。

### E04：文件上传字段

- **状态**：已执行；用户确认同字段多文件，以及上传图片/图片 URL 展示。实施、环境、命令、输出与限制见[0014 变更记录](../changes/0014-file-uploads-and-images.md)。
- **当前能力**：新增 file 字段、严格附件数组、同一 SQLite BLOB、会话暂存与配额/过期、事务绑定/替换/删除、受控上传及文件读取；新增 DSL image、SDK Image/AttachmentList 和表格/详情预览。契约见[多文件上传与图片展示](../contracts/file-uploads.md)。
- **执行验证**：路径穿越、跨会话/字段/记录、伪造元数据、数量/大小/MIME、失败回滚、取消与删除清理有回归；真实 CLI/HTTP、dev→run 附件读回和桌面/390px 浏览器已执行。没有修改或迁移用户数据库。
- **保留边界**：单文件上限 10 MiB，内联图片仅 PNG/JPEG/GIF/WebP；签名核对不是完整图片解码或病毒扫描。对象存储、分块续传和任意图片格式不在本轮承诺内。
- **相关入口**：[字段模型](../../crates/kenvoy/src/domain/model/mod.rs)、[附件数据库](../../crates/kenvoy/src/infrastructure/persistence/sqlite/uploads/mod.rs)、[上传 HTTP](../../crates/kenvoy/src/interfaces/http/uploads.rs)、[图片组件](../../crates/kenvoy/frontend/src/presentation/images.tsx)。

### E05：用户可选组件库兼容性

- **当前边界**：当前原生版本的第三方组件实证主要为 Radix/clsx；旧 MUI 阶段记录不能当作当前 MUI 兼容性验收。尚无实测不等于已经证明不兼容。
- **启用条件与范围**：仅对明确要承诺支持的用户库建立兼容用例。若包含用户自选 MUI，则在独立临时应用显式声明并锁定依赖；不得恢复框架默认 MUI/Emotion 依赖，也不因本项修改现有默认 UI。
- **验收标准**：实际构建及浏览器验证组件、Hooks/Props、样式、事件和适用的系统重载/数据绑定；原生标量、验证及权限契约不变；现状文档只引用实际执行过的当前版本证据。
- **相关入口**：[组件与选项契约](../contracts/component-bindings.md)、[构建回归](../../crates/kenvoy/frontend/tests/client-builder.test.mjs)、[扩展浏览器流程](../../crates/kenvoy/frontend/scripts/extensions-e2e.mjs)、[ADR 0010](../decisions/0010-native-components-and-data-bindings.md)。

### E06：发行、产物保留与恢复工具

- **已执行范围**：0.2.0 两个 crates.io 源包、六目标预编译 ZIP/SHA256、分发工作流与 cargo-binstall 基本安装核验已完成，见[0019](../changes/0019-crates-and-binary-release.md)；不扩大为各平台完整运行验收。
- **保留边界**：尚无二进制签名，历史构建版本不自动回收；迁移失败恢复有事务/备份基础，但没有通用自动恢复命令。
- **后续启用条件与范围**：按实际发行或运维需求补充供应链/许可证核对、签名、产物保留及恢复操作；另行确认范围，不将清理脚本直接用于业务目录。
- **验收标准**：发布包可追踪并在目标环境验证；只清理本项目拥有且非活动的产物，不误删当前/仍被使用版本、数据库或用户资源；恢复计划先预览、确认与备份，并验证数据完整性。
- **相关入口**：[发布操作](../operations/releasing.md)、[版本化产物](../../crates/kenvoy/src/infrastructure/artifacts/mod.rs)、[工程流程](engineering-workflow.md)、[迁移协议](../operations/migration-design.md)、[技术风险](technical-risks.md)。

## 不自动扩大为实施承诺

完整 TypeScript 类型检查、任意动态 Schema、完整 Node/Bun 兼容、多前端/数据库/HTTP/JavaScript 适配器、行级多租户等，不因本次归档而成为已承诺任务。出现明确需求时另行分析、确认范围并维护决策；不为未来可能替换而预建空适配层。

## 实施依赖与关闭要求

- I01/I02 先保留正确性基线与性能测量，再决定优化方案；I03/I04 需保持客户端脱敏和服务端可诊断两条边界。
- I05/E01 先确定迁移语义、数据保护和确认协议；E02/E03 先确定能力及隔离边界。可选项不得借依赖关系自动升级为已授权工作。
- V01/V02 可以在不改变功能设计的前提下独立补验；V03/V04 必须使用真实组件/API/数据库行为，不能只重钉实现文字或 mock 回声。
- 关闭 I/V 条目时，将状态改为已执行，记录代码范围、实际环境、命令、输出、退出码、结论和限制，并链接对应变更/验证记录；可选项只有明确纳入实施后才按同样要求关闭。
- 同步受影响契约、设计、路线、风险与索引；保留本页稳定编号和必要追溯关系。历史 ADR、变更与验证正文不回写，不将“已归档待办”写成“功能已实现”。
