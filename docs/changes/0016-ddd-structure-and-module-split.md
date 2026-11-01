# 0016：DDD 分层与模块拆分

状态：已执行；治理：不可变变更记录；日期：2026-11-02。影响范围：四成员 Workspace、框架前端及工程检查；前置：[ADR 0012](../decisions/0012-ddd-layers-and-module-boundaries.md)、[0015 示例](0015-uploads-example.md)；现状规范：[DDD 目录与模块规模](../architecture/ddd-structure.md)。

## 背景

用户要求按 DDD 驱动整理代码，严格划分目录、原子化与低耦合，及时拆分大文件。原实现包含技术模块平铺、领域模型依赖 DSL 来源类型、认证绑定具体数据库、HTTP 同时承担授权与发布协调，以及生产与测试职责混合的大文件。

本轮是行为保持的结构重构，不新增业务能力。两个产品与两个工程成员、默认原生 React/CSS、E04 附件/图片功能和四个正式示例均保留；未增加 Crate、兼容别名、事件总线或多后端空实现。程序版本不递增。

## 方案与不变量

- Kenvoy 使用 `domain/application/infrastructure/interfaces/bootstrap`；KEngine 使用语言域四层并保留既有公开根 API；前端使用 `app/domain/application/infrastructure/presentation`，保留真实 `main.tsx/sdk.ts` 入口。
- domain 保存定义、值对象和纯规则；application 组织编译、身份、资源、执行及发布，通过真实静态端口调用适配器。SQLite、SWC/QuickJS、系统凭据、项目文件与客户端工具链属于 infrastructure。
- HTTP 仅解析协议、防护请求并映射用例结果。授权与受控投影统一为应用用例；发布锁和已接受任务协调移入应用层；具体 HTTP 泛型绑定位于外层 wiring，bootstrap 完成进程装配。
- 保留原校验时机、权限单项匹配、数据库事务、取消/提交顺序及序列化。客户端切片通过真实 SWC 静态端口在原 check 阶段转换，不转移到 build；不为抽象增加 BoxFuture/虚拟仓储。
- 自有 Rust/TS/TSX/MJS/CSS（含测试）每文件最多 500 物理行，目标 400 行内。自然职责拆分，不建万能 shared、不以旧内部路径 shim 或宽可见性应付门禁。

## 实施文件清单

以下按迁移边界列出源码范围；目录中的实现和配套测试同步移动，逐文件增删/移动以本次 Git 提交为准。旧平铺实现已退出，不维护第二套入口。

| 文件或目录 | 实施内容 |
|---|---|
| `crates/kengine/src/lib.rs`、`domain/` | 保留公开门面，拆分 IR、来源位置、诊断、语义、模块与链接规则 |
| `crates/kengine/src/application/`、`infrastructure/`、`interfaces/` | 分离共用分析/增量/Host 端口、SWC/文件系统和 Engine/AnalysisSession 装配 |
| `crates/kengine/tests/` | 按语言、位置、模块与诊断拆分原大测试，保留公开消费方式 |
| `crates/kenvoy/src/main.rs`、各层 `mod.rs` | 薄入口与分层模块声明，移除旧根 namespace |
| `crates/kenvoy/src/domain/` | 模型、来源、校验、选项、授权与身份值规则；解除 DSL/数据库/HTTP 反向依赖 |
| `crates/kenvoy/src/application/compilation/` | DSL 分析、静态求值及客户端切片按职责拆分，注入真实 ClientCodeTransformer |
| `crates/kenvoy/src/application/identity/` | 身份服务、错误/视图与仓储/凭据端口，保留原密码和会话语义 |
| `crates/kenvoy/src/application/resources/` | 查询、附件和导航用例及 DTO/仓储端口；统一领域授权 |
| `crates/kenvoy/src/application/execution/`、`publication/` | 纯执行契约、代际发布与取消安全协调 |
| `crates/kenvoy/src/infrastructure/persistence/sqlite/` | Schema、迁移、查询、上传、身份、仓储和 worker 分离，事务与物理格式不变 |
| `crates/kenvoy/src/infrastructure/javascript/`、`identity/` | 真实 SWC/QuickJS 适配器和系统密码/随机/时间实现 |
| `crates/kenvoy/src/infrastructure/project/`、`artifacts/`、`frontend/`、`client_build/` | 文件项目、资产、内嵌源码、构建、开发镜像与工具链适配 |
| `crates/kenvoy/src/interfaces/cli/`、`http/` | 参数/响应、控制器、防护、开发代理及集中 wiring |
| `crates/kenvoy/src/bootstrap/` | 命令、构建、服务和开发生命周期装配，基础设施不回引全局聚合错误 |
| `crates/kenvoy/tests/` | 更新新内部入口、真实转换器注入与看门狗精确过滤路径 |
| `crates/kenvoy/frontend/src/main.tsx`、`sdk.ts`、`app/` | 保留公共入口，分离路由/会话与应用装配 |
| `crates/kenvoy/frontend/src/domain/`、`application/`、`infrastructure/` | 纯类型/规则、上传生命周期与客户端端口、同源 Fetch |
| `crates/kenvoy/frontend/src/presentation/` | 字段控件、资源表单、文件控件、记录/表格、Context、页面树、后台与 UI 拆分 |
| `crates/kenvoy/frontend/tests/`、`scripts/` | 迁移原生规则测试 import；更新构建/开发路径；修正 watcher 场景之间的客户端同步等待 |
| `crates/kenvoy-tests/src/`、`tests/` | 进程、夹具与协议按职责拆分，公共测试入口/协议保持 |
| `xtask/src/checks/`、`commands/`、`main.rs` | 工程职责拆分，增加 AST 层依赖、目录及文件规模门禁和七项边界回归 |
| `Cargo.toml`、`Cargo.lock`、`xtask/Cargo.toml` | 仅工程检查增加 syn 2.0.119（full/visit）及 KEngine 公共解析依赖；产品依赖边界保持 |
| `AGENTS.md`、`README.md`、`DESIGN.md`、`docs/` | 固定分层/拆分规则，迁移当前实现链接，补齐 ADR、现状规范、技术栈、测试与索引 |

前端内嵌 SOURCES、源码缓存、Vite 入口/白名单和受控镜像同时迁移。SDK/Context 不反向导入注册表页面树，表单上传仅依赖客户端端口。样式不重设计；构建后的 CSS 文件仍为 `index-gFcFeahG.css`。

## 可执行约束

`cargo xtask architecture` 继续核验真实 Cargo metadata、四成员、产品方向、发布限制与正常依赖闭包；新增源码检查由 Rust syn AST 与 KEngine 公共 TS 语法结构驱动。覆盖路径/别名/公开门面、测试 cfg、生产通配导入、HTTP actor 泄露、前端类型/再导出/动态导入及 SDK/Context 注册表回引，不把注释和普通字符串误当依赖。

同一入口检查规范目录和全部自有代码规模；`cargo xtask check` 统一执行架构与文档校验。它不是完整类型系统或行为证明，仍需编译、回归和真实宿主烟测。

## 验证过程中的修正

1. 迁移编译暴露缺失闭合符、局部 import 和目录 helper 同名遮蔽，均在整合阶段修复；未通过屏蔽告警或减小验证范围处理。
2. 严格 Clippy 暴露 KEngine 内部语言选择函数的大错误值、工程方法命名和不必要闭包。语言选择改为 Option，由原解析边界构造同一诊断，避免新增堆分配；其余按实际语义修正，未加 allow。
3. 第一轮七套浏览器中六套通过，watcher 在配置恢复后的输入保留断言失败。原测试在上一轮结构恢复仅等待服务端 ready，未等待浏览器完成描述同步，就开始新的配置故障场景；结构代际变化本来允许重挂表单。
4. 临时只读跟踪在同步后的复验中观察到服务端/客户端 generation 均为 3，输入仍为“配置恢复保留输入”；首轮失败时的客户端具体 generation 未采集，不推定其数值。永久修正仅让测试先确认旧诊断可见、恢复后诊断清除，再开始配置场景，保留原输入断言；未改 Runtime、未放宽状态保留要求。调试输出已删除，最终同一正式 CLI 的七套完整流程全部通过。

## 后续验证建议

后续新增代码先运行 architecture/check，超过目标区间时按职责拆分；公共接口或跨层修改继续执行相应运行回归。若修改密码/Session、附件格式、Schema、发布或转换时机，应重新做旧库和真实 CLI 比对，不能仅依赖目录门禁。

本次不新增跨平台、最低 Rust 版本运行矩阵、长时负载、断电/磁盘故障或真实原生 Ctrl+C 验收。这些仍按[既有待办](../development/implementation-backlog.md)与[技术风险](../development/technical-risks.md)管理，不因结构重构宣称关闭。

## 执行验证记录

### 环境与基线

- Windows x64；Rust/Cargo 1.99.1、Node 24.22.0、npm 12.2.0；依赖已准备，Cargo 命令使用 offline/locked。
- Git 基线为 `9816761`，本轮开始工作树干净。基线 CLI 先采集四例 inspect、help，并创建独占旧库夹具；没有使用或写入正式 examples 的数据库。
- 手动 UI 使用 omp 既有浏览器配置，实际版本 Chrome 155.0.8059.85；七套 Playwright 流程使用项目既有驱动与浏览器配置，未为采集版本另起浏览器或改配置。
- rust-analyzer 在本机启动后退出，LSP 不可用；本轮依赖定位采用已读源码、AST 与编译证据，未将 LSP 尝试写成成功验证。

### 命令与实际输出

| 验证 | 实际命令/操作 | 实际结果与退出码 |
|---|---|---|
| 前端 | `npm run check --prefix crates/kenvoy/frontend`、`npm test --prefix crates/kenvoy/frontend`、`npm run build --prefix crates/kenvoy/frontend` | 均退出 0；117 tests passed、0 failed；Vite 115 modules |
| KEngine 独立 | `cargo check -p kengine --offline --locked`；`cargo test -p kengine --offline --locked` | 均退出 0；66 项通过，已包含于 Workspace 计数，不另累加 |
| 严格静态/构建 | `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`；`cargo build -p kenvoy --offline --locked` | 均退出 0，无告警豁免 |
| Workspace | `cargo test --workspace --offline --locked` | 退出 0；256 项不重复常规测试通过，8 项默认 ignored；两个看门狗子进程复跑不重复计数，0 tests 不计入 |
| 原有显式依赖隔离用例 | `cargo test -p kenvoy --bin kenvoy --offline --locked real_custom_build_installs_only_explicit_user_dependencies_and_reuses_toolchain -- --ignored --nocapture` | 退出 0；1 passed、134 filtered；真实锁定工具链/用户依赖安装及缓存复用 |
| 完整浏览器最终复验 | `KENVOY_BINARY=D:/kenvoy/target/debug/kenvoy.exe cargo test -p kenvoy-tests --test browser --offline --locked -- --ignored --nocapture` | 退出 0；7 passed、4 filtered；forms/fields/admin/extensions/options/watcher/production 全通过 |
| 源码结构 | `cargo run --package xtask --offline --locked -- architecture` | 退出 0；四成员、正常产品依赖方向通过；336 个自有代码文件、272 个 Rust 文件、26 个前端模块；最长 449 行，上限 500 |
| 最终格式 | `cargo fmt --all -- --check` | 退出 0 |
| 统一结构/文档门禁 | `cargo run --package xtask --offline --locked -- check` | 退出 0；Cargo/AST/规模通过，82 个 Markdown、838 个本地链接通过 |
| CLI 基线 | 新旧程序分别对 customer/options/extensions/uploads 执行 `--project examples/<name>-app inspect`，并执行 `--help` | 五份 stdout 逐字节相同；分别 5547/14484/9085/10462/1021 字节，未归一化或删字段 |
| 旧库计划 | `target/debug/kenvoy.exe --project artifacts/ddd-legacy-U2J1jI migrate status` | 退出 0；from_version 1、changes 空、destructive false，from/to hash 均为 `709ffbe78d4f5c86951729b5478401601e39cfc485e7b199c7ae15b763f4adea` |
| 旧库运行 | `target/debug/kenvoy.exe --project artifacts/ddd-legacy-U2J1jI run --install-deps` 后真实 HTTP 与 UI 登录 | 原 Session 识别 legacyadmin，旧密码可登录；记录 1 的完整 JSON 与旧程序读回值一致；原 PNG 4615 字节、TXT 35 字节均逐字节一致 |
| 旧库 UI | 打开 `/gallery`，1440px 与 390px 截图及真实图片解码 | 显示“DDD旧库兼容夹具”、一张 640×360 图片和两个原下载链接；390px 无横向溢出 |

本机临时证据位于忽略目录 `artifacts/`：`ddd-before.json`、`ddd-parity.json`、`ddd-legacy-before.json`、`ddd-compatibility.json`、`ddd-legacy-gallery-desktop.png`、`ddd-legacy-gallery-mobile.png`。凭据不写入记录或提交；独占夹具与进程完成验证后清理，证据文件不属于发行物。

### 结论与已知限制

已验证内部目录、真实依赖方向和文件规模约束，以及本机现有公开契约/运行路径的兼容性。未把静态评审、格式化、忽略用例或历史平台报告当作新增功能证明；目录门禁不证明所有未来语义或并发路径正确。
