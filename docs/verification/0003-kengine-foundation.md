# 0003：阶段 01 KEngine 基础验收与探针退出保护

状态：已执行；日期：2026-11-01；前置：[阶段 00](0001-stage00-probes.md)、[跨工具验证](0002-axum-react-browser.md)。

## 实施结果

KEngine 已从空 Library 实现为独立通用解析库：Engine、JS/TS/JSX/TSX 解析、版本化 Generic IR、结构诊断、基础模块解析和静态依赖图。Kenvoy 添加本地路径依赖，其测试只消费公共 API，未引入业务模型或运行时实现。

正式源码为 lib.rs、engine.rs、parser.rs、source.rs、ir.rs、module.rs、diagnostics.rs。内部使用配套 SWC Parser/AST/Visitor，公共 API 不导出 SWC 类型；Generic IR 含源码、节点/表达式 arena、独立 span 和 UTF-16 字面值，未知表达式不执行，未知版本拒绝解码。

正式测试覆盖四语法、函数/方法/箭头/表达式 JSX、属性顺序与 spread、导入导出/type-only/重导出、解构声明、UTF-16 孤立 surrogate、BOM/CRLF/emoji 位置、恢复错误、读取失败、显式语言、相对/index/.diu/JS→TS 路径、根边界、缺失依赖、精确 external、SCC 循环与自环、版本 99 拒绝。Kenvoy 两项测试验证公共 API 消费与错误识别。

## 资源退出补充

独立代码审查发现阶段 00 辅助测试有三项资源退出缺口，本次修复而非隐藏：

1. 服务 JoinHandle 被移入 timeout 后可能 detach；改为借用等待、超时 abort/reap，并加 Drop 保护。
2. Node Child 在提前错误返回时可能遗留；增加 RAII 守卫，Windows 按仅本次创建的 PID 直调 taskkill /T /F，回收自有进程树；注入真正的提前 `?` 返回验证守卫。
3. JS 死循环探针若 interrupt 回归失效会拖住主测试进程；移入同一测试 executable 的子进程，父进程独立 deadline，并测试关闭 interrupt 的不合作子进程能被终止回收。

新增 shutdown_timeout_aborts_and_reaps_the_owned_server_task 故障测试，确认任务持有资源已 Drop。P05 事件接收先检查 deadline，事件队列持续有内容也不能绕过期限。它仍未覆盖文件删除、目录改名、事件风暴恢复或真实应用迁移协调。

## 环境与工具

主验证环境仍为 Windows/PowerShell、Rust/Cargo 1.99.1；用户已授权工程必要工具自主决定，本次额外安装 Rust 1.98.0 minimal（rustc 88d9e12ae 2026-08-18），未修改默认 stable 工具链或项目 override。

MSRV 测试使用独立 target/msrv-1.98 目录和临时 CARGO_INCREMENTAL=0，避免与主验证缓存混用。Node/npm/Chromium、SQLite 和锁定依赖沿用前两份报告。

## 实际验证命令与结果

| 命令 | 退出码 | 实际结果 |
|---|---|---|
| rustup toolchain install 1.98.0 --profile minimal --no-self-update | 0 | 安装最小三组件，rustc 1.98.0；默认工具链保持 stable |
| cargo +1.98.0 test -p kengine --offline --locked | 0 | 18 个正式 API 测试+6 个语言探针，24 passed / 0 failed |
| cargo check -p kengine --locked | 0 | KEngine 独立检查通过 |
| cargo test -p kengine --locked | 0 | 24 passed / 0 failed；unit/doc-tests 各 0 |
| cargo check --workspace --locked | 0 | Workspace 通过 |
| cargo test --workspace --locked | 0 | 常规 39 passed / 0 failed，跨工具 1 ignored；内部子进程测试输出不重复计数 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 | 无告警 |
| cargo fmt --all -- --check | 0 | 格式通过 |
| cargo test -p kenvoy --test p03_http_sqlite axum_serves_built_react_and_sqlite_to_real_chromium --locked -- --ignored --nocapture | 0 | 资源保护修复后显式复验，1 passed；Chromium 页面无错误，201/401/404 和 SQLite 重开1条通过 |

常规 39 项分布为 engine_api 18、p01_language 6、engine_boundary 2、p02_javascript 9、p03_http_sqlite 3、p05_watcher 1。跨工具测试虽然默认 ignored，本轮已明确执行，不能算成未验证或在常规统计里重复算通过。

此外使用 Node 读取 cargo metadata --locked --format-version 1 并检查普通依赖闭包，退出 0：仅 kengine/kenvoy 两个成员；KEngine 为 lib 无 bin，Kenvoy 为 bin 无 lib；路径依赖存在；KEngine 普通依赖闭包 93 包，无 Kenvoy/Axum/Tokio/rusqlite/rquickjs/Tower/notify。

定稿前再次执行 engine_api（18 项）和 engine_boundary（2 项）、workspace 严格 Clippy 与格式检查，均退出 0。Node 文档检查退出 0：共 33 份 Markdown（其中 docs/ 30 份）、112 个本地链接，UTF-8/行尾空白/索引可发现性 errors=[]。git diff --check 退出 0，仅提示既有 Git 配置将 LF 转 CRLF；没有修改该配置。node_modules、前端 dist/.probe 和 MSRV target 均命中 ignore。Git 仍仅包含本任务未提交文件，没有提交、推送、分支或标签变化。

严格 Clippy 首次发现两个大枚举，改为 NodeKind::Element(Box<ElementIR>)、Named 属性名称装箱，未关闭 lint，JSON 形状不变；修复后通过。部分代理定向编译仍观察到既有 incremental session 目录拒绝访问 note，命令退出 0；统一复验用临时 CARGO_INCREMENTAL=0 未再出现该 note，不宣称根因已修复。

## 验收边界与后续

阶段 01 的正式解析基础与公共 API 验收通过。完整作用域、SymbolId/ReferenceId、遮蔽/闭包/跨模块绑定、增量语义缓存、tsconfig/npm 条件解析和动态 import 依赖推断均未实现，属于后续阶段；direct_identifier 只是语法文本。
独立审查补清并测试两处边界：DeclarationIR.exported 仅为顶层 inline/default 声明导出语法标记，分离导出和 namespace 内符号需后续绑定；非法 UTF-16 说明符保留原 ImportIR/诊断但不能生成字符串图边，因此错误结果的图可能不完整。ExpressionIR 也是摘要与 raw 引用，不声称完整操作树。

没有 Kenvoy DSL、Application IR、正式数据库迁移器、生产认证、CLI dev/run 或 customer-app 闭环。下一阶段完成通用符号与引用系统及增量分析，再进入业务解释，保持双 Crate 单向边界。源码变更和文档索引参见 [变更记录](../changes/0002-kengine-foundation.md)。
