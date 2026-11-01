# 阶段 00 验证报告

记录日期：2026-11-01。性质：本轮执行事实；本报告定稿后不改写为后续阶段结果。

## 范围与环境

工作目录 D:\kenvoy；Windows、PowerShell；分支 main；HEAD b381cba。修改前 Git 工作区干净。无项目级 AGENTS.md、docs、README、CI 或测试配置；采用用户提供的全局规则和本轮阶段 00 明确范围。

实际版本输出：rustc 1.99.1 (58cf8f9c3 2026-10-13)、cargo 1.99.1 (5f94df478 2026-08-27)、Node v24.22.0、npm 12.2.0、Clippy 0.1.99、rustfmt 1.10.0-stable。项目声明 Rust 2024/MSRV 1.98；没有在 Rust 1.98 单独验证。

## 完成情况与文件变更

完成项目现状与历史路线审查、双 Crate 边界、编译/解释/运行生命周期、DSL/Application IR、数据库迁移、安全和技术风险设计；核对官方技术资料并制定实测门禁。

新增文档共 20 个：docs/README.md、architecture.md、technology-stack.md、crate-boundaries.md、compiler-design.md、runtime-design.md、dsl-design.md、database-design.md、technical-risks.md、development-roadmap.md、verification-report.md；docs/adr/README.md 及 0001–0008 共八篇 ADR。可通过 [总索引](README.md)与 [ADR 索引](adr/README.md)发现全部文件。

未修改或删除任何已有跟踪文件；Cargo.toml/Cargo.lock、两个 Crate 的源文件与依赖均保持原状。没有新增可调用公共 API，也没有创建任何业务数据库、前端资产或发布产物。编译产生的 target 属于已有 ignore 范围；没有提交、推送、切分支或恢复历史代码。

## 实际执行与观察

| 命令/检查 | 退出码 | 实际结果与结论 |
|---|---|---|
| git status --short | 0 | 修改前无输出，工作区干净 |
| git branch --show-current / git log -5 --oneline | 各 0 | main；HEAD b381cba，上一提交 0a84400 |
| git ls-files；读取现有 manifests、入口、ignore、lock | 0 | 8 个跟踪文件；lib 空、main 为空实现；无第三方依赖 |
| cargo metadata --offline --no-deps --format-version 1 | 0 | 两成员；kengine 为 lib、kenvoy 为 bin；dependencies 均为空 |
| cargo check --workspace --offline --locked | 0 | 两个包检查通过；只验证空骨架 |
| cargo test --workspace --offline --locked | 0 | kengine 0 tests、kenvoy 0 tests、doc-tests 0 tests；不存在功能覆盖 |
| cargo clippy --workspace --all-targets --offline --locked -- -D warnings | 0 | 骨架通过；不是业务质量验收 |
| cargo fmt --all -- --check | 0 | 格式检查通过 |
| ego-browser skill | 1 | CommandNotFoundException；默认浏览器能力不可用，未安装，改用现有 web 查询官方资料 |
| Node require.resolve 探针 | 0 | 探针脚本成功；七组包均 unavailable in workspace，不能理解为依赖测试通过 |
| python --version | 1 | 系统仅有 WindowsApps 别名，输出 Python was not found；没有运行 Python/SQLite 替代探针 |

Node 探针检查 react、react-dom、@mui/material、react-router、rolldown、vite、@playwright/test，均未能在本项目解析。命令返回 0 只说明逐项捕获并输出了真实缺失结果。

首次并行执行 Cargo 检查发生包缓存/构建锁等待，test 和 Clippy 输出 incremental session 目录拒绝访问的 note，但命令退出 0。随后在同一进程设置临时 CARGO_INCREMENTAL=0，顺序重跑 check、test、Clippy、fmt，全部退出 0，未再出现该 note。没有修改项目或用户 Cargo 配置。

顺序复验的关键输出如下；省略构建耗时和目标哈希，不改变结果：

```text
Checking kengine v0.1.0 (D:\kenvoy\crates\kengine)
Checking kenvoy v0.1.0 (D:\kenvoy\crates\kenvoy)
Finished `dev` profile
running 0 tests
test result: ok. 0 passed; 0 failed
running 0 tests
test result: ok. 0 passed; 0 failed
Doc-tests kengine
running 0 tests
test result: ok. 0 passed; 0 failed
Finished `dev` profile
```

后续阶段再次复验时，应保留本次首次 note 的事实，不把临时禁用增量说成原问题已经修复。

## 技术可行性证据边界

使用官方站点、上游仓库和 docs.rs 核对 SWC、rquickjs、Axum/Tokio、rusqlite、SQLite、notify、React/MUI/Router、Rolldown/Vite 和 npm 锁文件。链接、资料版本和推断见 [技术栈](technology-stack.md)与 ADR。资料查询不等于编译或运行这些依赖；在线文档的 latest/master 可能继续变化。

未执行：SWC 真实 TSX、Generic IR 序列化、Kenvoy DSL 转换、TS 去类型与 QuickJS ESM/Promise/限额、rusqlite 数据库及迁移/并发、Axum API/静态资源、React/MUI 页面、Rolldown/Vite Fast Refresh、notify 增量更新、Playwright E2E。未安装或升级任何工具、Rust/npm 依赖或浏览器组件。

仓库本身未接入这些依赖，且全局规则要求安装前明确批准；本轮仅授权阶段 00 审查和文档，不推定允许安装。技术集成实测仍是阶段 00 的待验收项，不以文档结果替代。

## 文档验收与遗留问题

文档检查使用 PowerShell here-string 向 `node --input-type=module` 传入临时脚本，遍历 Markdown、解析相对链接、从总索引递归检查可发现性、验证必需文件/ADR 章节、严格 UTF-8 解码和行尾空白。首次脚本退出 1：PowerShell 管道将脚本中的中文匹配常量变成问号，误报 ADR 章节缺失；文档 UTF-8 内容正常。改用 Unicode 转义常量后重跑退出 0，输出如下：

```json
{"markdown_files":20,"adr_files":8,"reachable":20,"local_links":50,"errors":[]}
```

`git diff --exit-code -- Cargo.toml Cargo.lock .gitignore crates` 退出 0，原有源码与配置未改变；`git diff --check` 退出 0，`git status --short` 仅显示未跟踪的 docs/。由于 git diff 不覆盖未跟踪文件，新增 Markdown 的空白/编码由上述 Node 脚本另外覆盖。

独立只读复核了全部 20 篇文档的一致性，补充了浏览器安全整数范围及匿名提交结果不确定时由管理员核对的规则。检查只验证文档结构、设计一致性与变更范围，不验证计划中的业务功能。设计 API、IR 和命令均明确标为待实现，本轮没有新增冒充已编译的 TSX/Rust 示例。

已知限制：阶段 00 设计已交付，但技术探针尚未通过；MSRV、Windows C 编译链、精确依赖兼容组合、JavaScript 限额和 React HMR 均未验证。当前不存在可供演示的 Web 应用或有效业务测试。

下一步是审阅阶段 00，形成明确的依赖批准清单并按 [路线](development-roadmap.md)补齐 P01–P05 及对应证据。阶段 00 通过后，阶段 01 从通用 Parser/IR 开始，不同时堆叠 HTTP、数据库和前端。
