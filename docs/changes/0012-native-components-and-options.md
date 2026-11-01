# 原生默认组件、系统重载与联动选项

状态：已执行。日期：2026-11-01。前置：[0011 首版验收](0011-final-acceptance.md)；决策：[ADR 0010](../adr/0010-native-components-and-data-bindings.md)。

## 背景与方案

用户要求及时提交 Git，扩展 select 为联动及跨表选项；允许重载系统组件，同时保持与后端的原生数据结构关联；移除项目 MUI 默认依赖，默认组件改用 React、TypeScript 和原生 CSS，应用可以选择 React 组件库。

先将已完成的首版实现与整理后的文档提交为 `e6b9ee0`，再实施本次独立功能变更。KEngine 不增加业务类型或依赖，Workspace 仍只有两个 Crate。

## 实施与接口

- `option.when` 声明本资源依赖条件，按字段规则规范化比较；支持数字、布尔、日期时间、可空值与多级依赖，拒绝循环。
- `select.optionsSource` 声明来源资源、值列、标签列、等值依赖映射和最低角色。专用选项接口有会话、Origin/CSRF、代际、表单策略、参数及分页限制，只返回 value/label。
- 来源 value 为 id 字符串或 unique 文本列；select 保存 string/null。创建和合并后的 PATCH 在 SQLite 写事务内重新验证来源记录和联动条件。低角色表单绑定高角色来源在 DSL 阶段拒绝，HTTP 写入也独立检查，避免猜值侧信道。
- `<override name="select" component={CustomSelect}/>` 使用普通客户端模块引用；支持七类字段和 form/table/group/page。公开 SDK 的 useField/useForm/useResource 共享原生字段、值、验证和受限 API，不复制第二份业务模型。
- 编辑草稿与原生值分开：非法数字、日期等保留 draft，绑定 value 为 null 且 valid 为 false；显式可空 null 保持有效。表单公开各字段有效性，无效依赖不参与选项查询或静态空值匹配，提交重新验证原始草稿。
- 默认 Runtime、管理后台、登录、导航、对话框、表格及字段全部改为语义 HTML/CSS。原生 dialog 保留焦点约束、Esc、返回触发器和处理中禁止关闭；窄屏管理入口保持可达。
- 表单错误、重置和成功反馈使用一次性布局阶段焦点请求，等待字段解除禁用后同步聚焦，避免延迟帧回调打断下一次输入。
- React 基础运行时继续统一解析，其他组件库从应用已声明依赖解析。Radix 仅用于用户示例/测试，默认资产没有组件库依赖；框架和探针锁文件不再包含 MUI/Emotion。
- 修复既有 JavaScript 队列测试使用固定 sleep 判断工作线程进度的竞争。使用可观测队列容量与首次 poll 同步，保留 500ms 恢复/关闭限制，并校验活动及排队调用都已关闭；生产 Runtime 行为没有因此放宽。

Application IR v1 的新增 component_overrides/options_source/when 均有缺省值，旧描述可读取。重载不改数据库类型；when/来源筛选只是运行规则，不产生存量清洗或级联更新。物理枚举约束改变仍走既有 Schema Diff。

## 文件变更

| 范围 | 新增或修改 |
|---|---|
| Rust 模型与 DSL | application.rs、options.rs（新增）、dsl/analyzer.rs、validation.rs、main.rs |
| SQLite | database/options.rs（新增）、mod.rs、query.rs、worker.rs、worker_tests.rs、migration.rs |
| HTTP | http/handlers.rs、mod.rs、projection.rs、tests.rs |
| 客户端构建与 JS | client_build.rs、client_build_tests.rs、project.rs、javascript/compiler.rs 的模型夹具、javascript/runtime/tests.rs |
| 前端 Runtime | src/bindings.ts、options.ts、ui.tsx（新增）；App/Admin/Runtime/main、api/sdk/types/validation、theme/styles |
| 前端依赖与验证 | package.json/lock、scripts/client-builder.mjs、options-e2e.mjs（新增）、forms/fields/admin/extensions E2E、tests/options.test.mjs（新增）、api/client-builder 测试 |
| 示例和探针 | 新增 examples/options-app；extensions-app 改为用户 Radix 依赖；tools/probes/frontend 改为原生控件并更新锁文件 |
| 文档与约定 | README、AGENTS、PRODUCT、DESIGN、surface；component-bindings、ADR 0010、本记录；架构/DSL/IR/数据库/迁移/安全/运行时/技术栈/路线及索引同步 |

未删除用户业务文件或迁移用户数据库。测试只创建并清理自身临时项目。历史 ADR、阶段验收和变更正文保留，当前索引标明默认 MUI 选择由新 ADR 替代。没有更改版本号、推送、分支或标签。

## 已知边界与后续建议

选项引用不等同数据库外键：来源删除或规则变化不会自动修复既有业务值；后续写入会重验。重载容器应保留 children，字段组件应将库返回的对象转换为原生标量。服务端始终执行最终类型和权限检查。

用户库需要兼容当前 React 且可在浏览器构建，不承诺任意包、Node API 或所有组件库都已验收。跨平台、长时负载及故障注入仍按原风险清单跟进。本次不重复声称验证了原生终端 Ctrl+C，历史证据保留在阶段 09。

## 执行验证记录

环境：Windows/PowerShell，Rust/Cargo 1.99.1，最低版本编译检查 Rust 1.98.0；Node 24.22.0，npm 12.2.0，Chromium 156.0.8078.4。依赖通过 npm registry 和精确版本/锁文件准备，安装使用 `--ignore-scripts --no-audit --no-fund`。未安装浏览器工具；ego-browser CLI 不存在，使用项目已有 Playwright。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo build --workspace --offline --locked` | 退出 0 |
| `cargo test --workspace --offline --locked` | 最终退出 0；205 项常规用例通过，2 项显式跨工具用例按默认策略忽略；看门狗子进程结果不重复计数 |
| `cargo test -p kenvoy --offline --locked -- --ignored` | 退出 0；真实自定义客户端安装/构建及 Axum/SQLite/Chromium 两项通过 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 退出 0；队列夹具修订另通过 Kenvoy 严格 clippy |
| `cargo fmt --all -- --check` | 退出 0 |
| `cargo +1.98.0 check --workspace --all-targets --offline --locked` | 退出 0，最低版本全部目标可编译 |
| frontend：`npm run check`、`npm test`、`npm run build` | 全部退出 0；101 项测试通过，包含非法草稿的类型稳定性，默认产物约 310 kB 主 JS、6.8 kB Admin JS、12.9 kB CSS（未压缩） |
| `node scripts/options-e2e.mjs` | 退出 0；静态/远程联动、分页/搜索/selected、Radix 重载、SDK 元数据、显式 null、对象输入拒绝、无效数字草稿的稳定类型与禁止提交、修正后真实写入、服务端伪造拒绝、PATCH 合并校验、后台编辑、移除重载后的原生控件及重启持久化 |
| `node scripts/forms-e2e.mjs`、`fields-e2e.mjs`、`admin-e2e.mjs` | 均退出 0；真实登记、七类字段、后台 CRUD/搜索/筛选/排序/分页及新增原生弹窗/菜单键盘和焦点断言 |
| `node scripts/extensions-e2e.mjs`、`production-e2e.mjs`、`watcher-e2e.mjs` | 均退出 0；Radix 用户组件、Hooks/CSS HMR、私有模块隔离、生产自动构建、仅产物运行、监听及错误恢复 |
| `node tools/tests/http-cli.mjs` | 退出 0；20 项真实 HTTP 断言通过 |
| `kenvoy --project examples/options-app check`、extensions-app check | 均退出 0；声明与模块引用有效 |
| probes/frontend：`npm run build` | 退出 0；原生探针资产供显式跨工具测试使用 |
| `node tools/verify-architecture.mjs`、`node tools/verify-docs.mjs` | 退出 0；两包单向边界、UTF-8、本地链接及文档可达性通过 |
| `impeccable detect --json crates/kenvoy/frontend/src crates/kenvoy/frontend/index.html` | 退出 0，结果 `[]`；登记/后台/选项的桌面与 390px 截图已实际查看，视觉复核 ready |

初次全量 Rust 测试因队列夹具固定等待返回 Busy 而退出 101，修复确定性同步后定向连续 20 次及全量复测通过。初次后台浏览器测试因筛选标签不明确失败，补 aria-label 后原断言通过。新增 E2E 夹具一度缺少函数闭合括号，修正后完整运行通过；未关闭校验或删除失败断言。

最终字段契约复核新增 3 项测试，前端总数由 98 增至 101。浏览器复测曾出现 Chromium 截图协议失败；串行复测又捕获延迟聚焦造成快速输入落入错误字段的问题。改为一次性布局阶段聚焦后，重新构建并串行执行 forms、fields、admin、options 全部退出 0，保留原有断言。测试夹具新增 number 重载，实际验证非法草稿不发送写请求，修正后仅提交一次且 SQLite 返回 number。

部分 Rust 命令有既有 incremental 缓存目录“拒绝访问”提示，影响缓存复用，命令最终仍退出 0；没有修改权限或删除缓存规避。验证日志位于本地 artifacts，未纳入 Git；历史截图与记录不冒充本次新功能验收。
