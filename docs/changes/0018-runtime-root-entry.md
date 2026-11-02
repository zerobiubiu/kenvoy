# 0018：Runtime 根入口修复

状态：已执行；治理：不可变变更记录；日期：2026-11-02。影响范围：dev/run 的 HTTP 页面入口；前置：[0017](0017-windows-password-prompt-encoding.md)；现状：[前端 Runtime](../architecture/frontend-runtime.md)、[CLI](../operations/cli-design.md)。不修改 IR/数据库格式、依赖或产品版本。

## 背景与原因

用户启动 `examples/uploads-app dev --install-deps` 成功，访问日志中的应用根地址 `/` 却得到 `KV5004`“请求的资源不存在”。该输出作为失败证据，没有对用户运行中的服务重复请求，也未使用用户数据库作为验证夹具。

示例声明 `/uploads`、`/images` 和 `/gallery`，没有声明 `/`。前端 App 已有默认根路由：按当前身份过滤后的客户端描述进入首个可访问页面；没有可访问页面则去 `/admin/login`。宿主的应用导航用例只接受声明页及管理入口，提前拒绝 `/`，导致既有前端逻辑无法加载。缺口属于通用宿主，不通过给单个示例增加重复根页面或硬编码 `/uploads` 规避。

## 方案与安全边界

只在页面用例的声明页/管理入口匹配之后，允许未声明的精确根路径 `/` 返回 Runtime 页面壳。显式根页依旧先校验其声明权限；不新增导航策略或第二套权限选择。

- 未声明根页：GET/HEAD `/` 提供 HTML，客户端使用已有授权投影确定落点。
- 显式受限根页：匿名仍为 401，普通用户仍为 403，管理员按原规则访问。
- 未知页面、`/api`、`/_kenvoy`、未知 API 与内部路径仍返回真实 404；私有文件和路径穿越不进入 SPA。
- 不开放 Resource/附件权限，不将任意未知请求兜底成成功 HTML；原请求方法、防护、构建等待与 Vite 代理边界保持。

## 实施文件清单

| 文件 | 修改 |
|---|---|
| `crates/kenvoy/src/application/resources/navigation.rs` | 保留权限优先级，仅放行未声明的根入口 |
| `crates/kenvoy/src/interfaces/http/tests/routing.rs` | 扩展根 GET/HEAD 和未知路径回归，新增显式根页的匿名/用户/管理员权限测试 |
| `examples/uploads-app/README.md` | 明确 `/` 为入口，默认进入 `/uploads` |
| `docs/architecture/frontend-runtime.md`、`security-design.md`、`runtime-design.md` | 同步内建根入口与授权/开发代理边界 |
| `docs/operations/cli-design.md` | 说明启动日志根地址及无可访问页面时的行为 |
| `docs/README.md`、`docs/changes/README.md`、`docs/verification/README.md`、本文 | 增加实施与验证入口，保留既有定稿记录 |

## 执行验证记录

### 环境与数据边界

Windows x64；基线 `15b3ded`，本轮开始工作树干净。用户旧 dev 曾占用原 debug 可执行文件；由操作者选择正常停止，本轮确认原 PID 已退出后才更新程序，没有强制结束用户进程。LSP 启动退出，引用定位采用已读源码及编译证据。

真实程序验证使用仅复制源码/样例/配置的独占 `artifacts/root-route-9ad9e203` 夹具，单独创建测试账号与数据库；不复制用户 `data`、`dist` 或 `.kenvoy`，不访问用户的 3000 端口作测试。夹具使用独立 loopback 地址和随机端口，避免覆盖用户应用的浏览器会话 Cookie。

### 命令与实际结果

| 命令 | 实际结果 |
|---|---|
| `cargo fmt --all -- --check` | 退出 0 |
| `cargo test -p kenvoy --bin kenvoy --offline --locked interfaces::http::tests:: -- --nocapture` | 退出 0；19 passed，117 filtered；包含根 GET/HEAD、显式根页权限优先级及已有 HTTP 防护回归 |
| `cargo clippy -p kenvoy --all-targets --offline --locked -- -D warnings` | 退出 0，无新增豁免 |
| `cargo build -p kenvoy --offline --locked` | 退出 0，更新原 `target/debug/kenvoy.exe` |
| `target/debug/kenvoy.exe --project artifacts/root-route-9ad9e203 dev --install-deps` | 启动成功，`http://127.0.0.77:50818`，真实构建 6 个文件；仅准备夹具的锁定工具链 |
| `target/debug/kenvoy.exe --project artifacts/root-route-9ad9e203 run` | 复用同一夹具产物启动成功，`http://127.0.0.77:49415`，生产静态入口通过 |
| `node artifacts/root-route-browser-smoke.mjs http://127.0.0.77:50818 dev` | 退出 0；根请求 200，匿名及登录后的落点均为 `/uploads`，表单显示、真实登录、图库空状态通过，pageErrors 为空 |
| `node artifacts/root-route-browser-smoke.mjs http://127.0.0.77:49415 run` | 退出 0；相同根入口、匿名/登录身份和图库断言在生产静态模式通过，pageErrors 为空 |
| `cargo run --package xtask --offline --locked -- check` | 退出 0；Cargo/AST/规模通过，84 个 Markdown、853 个本地链接通过 |

两个模式分别执行实际 GET/HEAD `/`，均为 200 HTML，HEAD 正文为空；`/api/missing`、`/_kenvoy/missing`、`/not-a-page`、`/data/app.sqlite` 均为 404/KV5004，匿名 `/api/materials` 仍为 401。上述请求不包含用户项目地址。

默认 omp 管理浏览器启动失败（退出码 21）；经用户明确授权，改用项目已安装的 Playwright/Chromium 156.0.8078.4、独立临时上下文验证，未修改 omp 配置或操作用户现有标签。两个模式均从完整根地址打开，确认地址实际转为 `/uploads`、私有导航对匿名隐藏，并检查桌面截图；不是仅通过 API 或客户端内部跳转推定成功。

本机忽略证据为 `artifacts/root-entry-http.json`、`root-entry-dev.json/png`、`root-entry-run.json/png`。临时脚本、自有数据和进程均已清理，截图不属于发行资产。

### 覆盖限制与后续建议

本次不更改前端组件、样式或默认导航算法，不据此声称完成跨平台/全量浏览器矩阵。后续修改入口时应继续同时验证缺省根入口、显式根页权限、未知 API 的真实 404 和开发代理/生产静态模式，不能只检查客户端内部导航。
