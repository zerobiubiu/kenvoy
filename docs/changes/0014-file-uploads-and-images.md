# 0014：多文件上传、图片组件与持久化验收

状态：已执行；治理：不可变变更记录；执行日期：2026-11-02。影响范围：Kenvoy DSL/Application IR、SQLite、HTTP、React Runtime、SDK 与工程文档。前置：[0012 原生组件与数据绑定](0012-native-components-and-options.md)、[0013 Rust 工程入口](0013-rust-test-workspace-and-tools-retirement.md)。本次未发布、未修改产品版本、未推送远端。

## 背景与范围

用户要求实现待办 E04 文件上传字段，并增加能显示图片的组件。经结构化确认：同一字段支持多文件；图片支持已上传文件及图片 URL。默认 React 原生控件/CSS 与四成员 Workspace 不变，没有新产品 Crate、依赖或用户数据库迁移。

本轮实际模式切换验收同时取得 V01 所需的 dev 写入、直接 run、读取与编辑原记录、同库读回和生产接口边界证据；其他未完成条目不自动关闭。

## 方案

- 新增 `file` 字段与严格的 `UploadedFile[]`，复用原 Resource、字段投影、权限与 CRUD。标量字段不因扩展而接受任意对象。
- 采用同一 SQLite 的 BLOB 附件表：会话摘要归属暂存，写事务核验和绑定；替换、清空、记录删除及已确认 Schema 删除与附件清理同事务。沿用数据库一致性备份，不创建公开文件目录。
- 上传在接收大请求体前执行 Session/Origin/CSRF/代际/字段策略。暂存仅 owner 可读；已保存附件按资源与字段 Read 权限读取，公开新增不授予读取权。
- 新增 DSL `image`、SDK `Image/AttachmentList/fileUrl` 以及 file/image 重载。图片 URL 只允许显式安全路径或 HTTP(S)，失败有可见说明；附件预览仅使用服务器识别的 PNG/JPEG/GIF/WebP。
- 框架源码嵌入清单同时包含四个新前端模块，避免内置 Runtime 能运行、用户 SDK 自定义构建却缺模块的分叉。使用契约集中在[多文件上传与图片展示](../contracts/file-uploads.md)。

## 实施文件清单

| 分组 | 文件与实施 |
|---|---|
| 业务模型/声明 | `crates/kenvoy/src/application.rs`、`dsl/analyzer.rs`、`validation.rs`、`options.rs`：文件约束、元数据数组、Image、严格 DSL/URL/选项依赖边界与内嵌回归 |
| 数据库 | `database/mod.rs`、`worker.rs`、`query.rs`、`migration.rs`：actor 接口、JSON TEXT、事务绑定/替换/清理、旧标量 hash 兼容及迁移预检；新增 `database/uploads.rs` |
| 数据库回归 | `database/worker_tests.rs`、新增 `database/uploads_tests.rs`：跨归属拒绝、伪造、持久化、回滚、配额/过期、DDL 漂移、迁移清理与备份 |
| HTTP/产物 | `http/mod.rs`、`handlers.rs`、`projection.rs`、`tests.rs`、新增 `http/uploads.rs`；`artifact.rs` 接入 Image 来源迁移；`client_build.rs` 嵌入新模块 |
| 前端类型与行为 | `frontend/src/types.ts`、`api.ts`、`bindings.ts`、`options.ts`、`validation.ts`、`Runtime.tsx`、`Admin.tsx`、`sdk.ts`、`styles.css` |
| 新前端模块 | `frontend/src/files.ts`、`uploads.ts`、`FileField.tsx`、`images.tsx`：附件协议、表单独占状态机、原生多选和图片展示 |
| 前端回归 | `frontend/tests/api.test.mjs`、`validation.test.mjs`、`options.test.mjs`、新增 `uploads.test.mjs`：无重试/未知结果、取消/迟到响应、提交中卸载保护和严格数组边界 |
| 文档 | 根 `README.md`；文档中心/契约/变更/验证索引；`application-ir.md`、`dsl-specification.md`、`component-bindings.md`；新增 `file-uploads.md`；数据库/安全/前端现状；路线、风险及待办 |

上述省略前缀的 Rust 文件均位于 `crates/kenvoy/src/`。没有改动 KEngine 实现、Cargo/npm 清单或锁文件；新增测试只验证行为与边界，不固定源码文本或转发细节。

## 执行验证记录

### 环境

Windows 11 x64；Rust 1.99.1、Cargo 1.99.1、Node 24.22.0、npm 12.2.0；现有 npm/Cargo 依赖，Cargo 使用 offline/locked。浏览器通过 omp 当前默认配置启动 Chromium；未覆盖浏览器启动配置、未安装新浏览器。

所有写入均在本轮独占的临时项目 `artifacts/uploads-image-e2e-20261102` 或测试自有临时目录；没有使用现有示例或用户数据库。自定义客户端首次以 `--install-deps` 显式准备锁定工程工具缓存，不改用户应用依赖声明。

### 构建与回归

| 实际命令 | 实际输出/退出码 | 结论 |
|---|---|---|
| `npm run check --prefix crates/kenvoy/frontend` | `tsc --noEmit`；0 | 前端类型通过 |
| `npm test --prefix crates/kenvoy/frontend` | `tests 117`、`pass 117`、`fail 0`；0 | 前端边界/状态回归通过 |
| `npm run build --prefix crates/kenvoy/frontend` | Vite 8.3.5，108 modules transformed，built；0 | 内置资产生成成功 |
| `cargo build -p kenvoy --offline --locked` | Finished dev profile；0 | 实际 CLI 构建成功 |
| `cargo test --workspace --offline --locked` | 主套件合计 249 passed、0 failed、8 ignored；0 | Workspace 已执行回归通过；独立 watchdog 子进程的两项重复计数不再累加 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | Finished dev profile；0 | 严格告警门禁通过 |
| `cargo fmt --all -- --check` | 无差异；0 | 格式通过 |
| `cargo run --package xtask --offline --locked -- check` | 架构“通过”；78 篇 Markdown、779 个本地链接“通过”；0 | 四成员/单向产品依赖与文档入口通过 |

8 个 ignored 包含 7 个既有浏览器流程和 1 个真实用户依赖安装测试，未将其计为本轮执行。本轮额外完成下述 omp 浏览器专项，不冒称完整七套浏览器回归已重跑。

新增 Rust 回归实际覆盖：附件跨会话/字段/资源/记录拒绝、元数据伪造、JSON/文件不同 body 限额、超过 2 MiB 的合法上传、CSRF/代际、暂存取消、记录约束失败回滚、替换/清空/删除、过期/数量与字节配额、DDL/版本损坏、重启持久化、迁移清理及变更前备份。

### CLI、真实 HTTP 与 dev→run

1. 实际 CLI 创建独占项目，通过受控 stdin 初始化临时管理员，配置唯一 `data/app.sqlite`，不在命令行放密码。
2. `kenvoy --project artifacts/uploads-image-e2e-20261102 dev --install-deps` 启动开发服务；临时 HTTP 脚本在 `http://127.0.0.1:52846` 输出 `phase: initial, checks: 29, record: 1`，退出 0。
3. 29 项断言覆盖路径型文件名、伪造 image/SVG、他人暂存读取/绑定、匿名新增后的读取拒绝、管理员真实图片字节与 inline、强制下载、PDF attachment、PATCH 删除旧文件、唯一冲突保留暂存、主动取消和随记录删除。
4. 停止 dev 后，未先执行 build，直接 `kenvoy --project artifacts/uploads-image-e2e-20261102 run`；框架按指纹自动更新产物并启动 `http://127.0.0.1:59194`。临时脚本输出 `phase: restart, checks: 6, record: 1`，读取 `蓝色示例.png` 4615 B 与 `说明文档.pdf` 57 B，退出 0。
5. 生产无 Cookie 读取 `/api/documents/1` 和该已绑定文件均为 401；`/_kenvoy/development` 与 `/_kenvoy/vite/hmr` 均为 404。没有把文件 ID 当访问凭据，也没有生产 HMR。
6. 浏览器在 run 编辑 dev 创建的记录 3 并替换 PDF：提交前旧文件仍为 200，提交后旧 ID 为 404，新引用已绑定。直接查询同一 SQLite：`SELECT id,title,json_array_length(attachments) AS files FROM documents ORDER BY id` 返回记录 1/`HTTP附件持久化`/2 个附件，以及记录 3/`浏览器多文件与图片`/3 个附件。数据目录仅 `app.sqlite` 与其 WAL/SHM，没有第二份库。

这完成本场景的 V01 交叉验收，不等同于 V02 原生退出、V05 跨平台或 V06 全故障验证。

### 实际界面

- 基础页面通过外部测试 Origin 的 PNG URL 显示 640×360 图片；并非以占位图冒充上传数据。
- 空提交出现文件及文本必填错误；通过原生 multiple 输入上传两张 PNG 和 PDF，实际 naturalWidth 分别为 640、320。逐项移除后原 ID 为 404，重新选择可继续提交。
- 匿名提交成功生成记录 3，已保存文件不再显示无权访问的下载/预览入口；服务端仍独立拒绝未授权读取。
- 1440px 桌面与 390px 窄屏检查了同一原生上传组件，窄屏文档没有横向溢出；管理员列表、编辑回显和替换通过真实界面完成。
- 用户自定义 Gallery 从 `@kenvoy/client` 导入 `useResource/Image/fileUrl`，真实构建并显示三张已上传图片；图片尺寸读回正确。
- 故意 404 图片出现“图片未能加载，请检查地址或访问权限”；取消一次填写后，三个暂存 URL 均为 404。
- `impeccable detect --json` 对 FileField、images、Runtime、Admin 和 styles 执行一次，退出 0，实际输出 `[]`。没有修改既有设计系统或增加忽略规则。

本机截图为 `artifacts/uploads-desktop.png`、`uploads-mobile.png`、`uploads-admin.png`、`uploads-gallery.png`、`uploads-image-failure.png`；已逐张打开确认不是空白或错误页面。测试图片为本轮生成的合成矩形 PNG，只作功能夹具，不是产品素材；截图及非敏感 HTTP 摘要留在本机忽略目录，不入库。验收结束后已关闭自有浏览器标签和服务，移除两个独占项目及临时脚本，不把验收脚手架作为工程入口。

### 独立复审与设计对照

后端 reviewer 在指定附件/HTTP/数据库范围只读复审，未发现需修复的实质问题；前端 reviewer 逐张检查五张截图及上传状态/绑定，结论为 `ship`，仅适用于本轮功能，不代表全站或完整无障碍验收。两者均未重复运行构建或浏览器。宿主未列出技能专用完成复审/文档代理，改用独立 reviewer 检查相同证据及设计漂移；现有颜色、布局和 token 继续适用，`DESIGN.md` 与设计 sidecar 保留，不借扩展修复历史工具漂移。

### 失败与处理

- 首轮编译暴露 chrono 未启用 clock、rusqlite 不支持 u64 SQL 参数/读取，以及 Image 的产物位置匹配遗漏；分别改用既有 std 时钟、受限整数转换和完整枚举分支，未为通过构建扩大依赖 feature。
- Windows 运行中的 CLI 曾阻止 Cargo 覆盖 exe（os error 5）；先停止本轮自有服务，再完成编译、测试和严格 Clippy。PTY 外层退出 512 未被写成原生 Ctrl+C 优雅退出证明。
- 自动化曾在登录尚未完成时主动导航，并出现 CSS `ERR_NO_BUFFER_SPACE`；该次不计通过，生产现场按真正登录完成条件验证成功。编辑时另修正了把后台列表与表单附件一起计数的自动化条件，没有因此改产品逻辑。
- rust-analyzer 启动后异常退出，引用查询不可用；使用结构化/定向搜索、完整编译、回归与真实运行核对调用点，没有修改本机 LSP 配置。

### 已知限制与后续验证建议

单文件硬上限 10 MiB，默认最多 5 个、可配置至 20 个；只识别四类栅格图片签名，不承诺完整解码、转码/缩略图、病毒扫描、任意图像格式、分块续传或对象存储。暂存为整文件有界接收；大项目长期磁盘空间和压力需按部署目标另测。

本轮没有重新完成跨平台/浏览器/辅助技术矩阵、断电/磁盘满/网络盘故障、发布中途退出或全部开发代理导航场景；保留原风险，不把正常附件流程通过扩大为全部生产安全认证。用户部署时应按实际来源、容量与反向代理再次验收。
