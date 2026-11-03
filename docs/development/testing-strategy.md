# 测试与验收策略

状态：参考（现状文档）；按实际行为分层，不以模块数量、空测试或探针替代产品验收。

| 层级 | 实际位置 | 核心证据 |
|---|---|---|
| 语言 | crates/kengine/tests | TSX、表达式、词法作用域、模块链接、位置与增量 |
| DSL/验证 | kenvoy/src/application/compilation、domain/validation、tests/dsl_security.rs | 静态结构、业务冲突、边界和共享标量向量 |
| SQLite | kenvoy/src/infrastructure/persistence/sqlite 下的分模块测试 | 临时文件、事务、迁移、并发、关闭与重开 |
| 认证/HTTP | application/identity/tests.rs、interfaces/http/tests | 哈希/会话、权限、CSRF、限流、静态隔离、真实 TCP |
| CLI 独立测试 | kenvoy/tests | 真实 Binary、受控 stdin、配置与迁移 |
| 跨 Crate | kenvoy-tests/tests | KEngine 公共分析与 Kenvoy CLI、真实 HTTP/认证及持久化 |
| 前端 | kenvoy/frontend/tests | 与 Rust 共用 68 向量、API 状态和失败语义 |
| 浏览器 | kenvoy-tests/tests/browser.rs + kenvoy/frontend/scripts | Rust 管理 CLI/临时项目/进程，Playwright 执行真实 DOM 断言 |
| 结构门禁 | xtask/src/checks/architecture | 真实 Cargo 图、AST 依赖/别名/门面/测试 cfg、前端注册表隔离与文件规模 |

完整 Rust Workspace 验证前先构建框架内置前端：

```text
npm ci --ignore-scripts --prefix crates/kenvoy/frontend
npm run check --prefix crates/kenvoy/frontend
npm test --prefix crates/kenvoy/frontend
npm run build --prefix crates/kenvoy/frontend
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo fmt --all -- --check
cargo xtask check
cargo xtask test
cargo xtask browser
```

依赖已准备时可加 --offline。xtask 内部 Cargo 命令使用 offline/locked；test 先构建正式 CLI 并提供 KENVOY_BINARY 路径，避免误用旧二进制。KEngine 单独 check/test 不需要前端或数据库。npm 安装不下载 Playwright 浏览器；真实浏览器验证还需已准备的兼容 Chromium。当前使用的 Chromium 版本以各报告为准。完整接口见[工程与测试流程](engineering-workflow.md)。

测试只创建唯一临时项目及数据库，清理限于本测试拥有的目录和进程。网络测试绑定 loopback/系统空闲端口。API mock 只测试客户端错误处理，不能证明真实数据闭环；浏览器验收必须连接真实 Kenvoy。Windows 子进程 SIGTERM 是强制终止，不能将其称为 Ctrl+C 优雅退出。

所有阶段报告记录执行环境、命令、退出码、失败与修复、未覆盖项目。历史 `p01..p05` 是技术探针；其中依赖已退役 tools 前端的浏览器用例移除，由测试成员中的七套正式应用流程接替。其余 Rust 独立测试保留。显式忽略的跨工具测试必须单独调用，不能计入普通 cargo test 的通过数量。严格 Clippy 的剩余告警不能用 allow 或公开无意义接口掩盖。

跨层重构另保留旧 CLI 基线，逐字比较示例 `inspect` 与 `--help`；用重构前真实程序创建独立 SQLite/会话/附件，再由新程序读取并比较记录与文件字节。结构门禁不能替代运行回归；本轮实际证据见 [0016](../changes/0016-ddd-structure-and-module-split.md)，不据此扩大平台或真实 Ctrl+C 验收范围。

最终 E2E 已覆盖完整后台、自定义 UI/Server Function、错误恢复、热更新、生产自动构建与 artifact-only 部署；专项终端测试另验真实 Ctrl+C。阶段 10 的命令、计数与平台边界见[首版验收](../verification/0012-final-acceptance.md)，本次 Rust 工程入口的实际验证见[0013 变更记录](../changes/0013-rust-test-workspace-and-tools-retirement.md)，不以阶段 06 的初次表单证据替代最终结果。

## 正式迁移与兼容性验收

正式交付链的跨层入口为 `crates/kenvoy-tests/tests/migration_contract.rs`，场景按 catalog、transactions、runtime、adoption 分拆；低层执行器和冻结 Schema 断言位于 SQLite 模块测试。沿用 `cargo xtask test` 构建真实 CLI 并传递 `KENVOY_BINARY`，不得因旧二进制恰好存在而误验。

| 场景 | 需要观察的事实 |
|---|---|
| 空库和有数据库升级 | 按步骤到达目标结构；原记录、约束、索引和 ID 高水位正确 |
| 同 Schema 数据迁移和重复启动 | ID/checksum 推进，数据转换仅执行一次；不靠 Schema hash 猜测已执行 |
| 历史异常 | 改写、缺失、重复、分叉、实际结构漂移和数据库领先均在对外服务前失败 |
| 双重兼容 | 最低 CLI 条件通过但项目格式不支持仍拒绝；缺字段与预发布边界明确 |
| 冻结数据可读性 | 存量 CHECK/NOT NULL、系统 ID/时间与 UTF-8 直接核验；非法文本不能提交后才让 API 报错 |
| 未固化 Schema | run/dev/build 不修改数据库、不自动生成执行方案 |
| 分步失败恢复 | 当前步回滚，前缀保持提交；修复前提后从真实位置继续，不改历史文件 |
| 迁移成功但应用失败 | 端口占用等故障不能伪称回滚；启动日志区分 migrated、startup_failed 与 ready |
| 进程互斥 | 同库第二服务、迁移写入被拒绝；第一服务继续有效，退出后新实例可取得使用权 |
| 无源码交付 | 去掉源码/迁移目录且 PATH 无 Node 后，完整产物仍可给空库执行全链 |
| 基线与开发快照 | 接入证明/确认/旧历史保留；空快照不复制正式数据；单文件复制不丢失来源，不能借 baseline 转正 |

测试只能使用独占临时项目和数据库；备份恢复不覆盖用户业务库。浏览器 fields/options/watcher 夹具须显式固化 Schema 变更，watcher 区分 UI 热更新和停服迁移，不能把旧“启动时动态 Diff”断言重新固定为正式行为。实际执行命令、结果与未覆盖故障见[实施记录 0026](../changes/0026-application-migration-lifecycle.md)。

## 原生国际化验收

词典配置和只读工具回归位于 `crates/kenvoy/tests/i18n_cli/`，跨层入口为 `crates/kenvoy-tests/tests/native_i18n.rs`，场景在 `i18n_support/`；前端纯规则位于 `frontend/tests/i18n*.test.mjs`。沿用上文工程入口和唯一临时项目，不操作用户词典或业务库。

必须分别核对：旧未启用配置保持单中文；显式格式/最低版本门禁；重复/损坏/缺失词典及排除目录拒绝；默认 DSL 提取不被旧翻译反向覆盖；授权词条不泄露；词典改动改变构建身份但不改变 Schema 或迁移；格式 3 产物在无源码和词典目录时仍能运行。

浏览器覆盖自动语言与切换器、中文/英文/RTL、表单草稿和自定义 Hooks 保留、数字/日期样式、参数转义、内置错误与实际规范写入。仅打包成功不能证明自定义 dev 可用：真实浏览器必须成功加载内置 JSON 的 Vite 模块，并保持任意用户 JSON/私有文件拒绝。现有浏览器夹具明确设置 zh-CN，不能让系统语言漂移改变旧断言，也不把忽略的浏览器套件计入普通 Rust 通过数量。

本次真实命令、失败修复、桌面/窄屏范围及未覆盖项见[实施记录 0027](../changes/0027-native-project-i18n.md)；不以历史阶段计数替代当前结果。
