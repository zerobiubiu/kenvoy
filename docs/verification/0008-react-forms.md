# 阶段 06：通用 React 表单与真实浏览器验证

状态：已执行；日期：2026-11-01；前置：[HTTP 与认证](0007-http-auth.md)。

## 环境与范围

Windows、PowerShell、Node.js 24.22.0、Rust 1.99.1；真实 Kenvoy Binary、Axum 与文件 SQLite。前端使用 React 19.3.1、React DOM 19.3.1、MUI 9.5.0、React Router 8.5.1、Vite 8.3.5、Rolldown 1.2.16、TypeScript 7.0.2。浏览器通过 Playwright 1.64.1 驱动 Chromium 156，不使用模拟 API 或内存数据库。

本阶段验证通用页面/表单、管理员登录、真实提交回显、认证后的同记录读取，以及桌面和 390 像素移动视口。完整 Admin 列表、编辑、删除、详情、浏览器重启持久化、生产 run、文件监听、Fast Refresh、自定义组件与 Server Function 尚未由本记录验收。

## 依赖与构建

在 `crates/kenvoy/frontend` 执行必要的项目依赖安装，沿用已验证探针的前端技术版本，补充 TypeScript 7.0.2、`@types/react` 19.3.0、`@types/react-dom` 19.3.0。具体直接及传递依赖由该目录 `package.json`、`package-lock.json` 固定，来源和完整性信息保留在锁文件。框架源码开发需要 Node/npm；发行 Binary 内置通用资产，基础 DSL 应用启动不安装 npm 依赖。

本次 `npm install --ignore-scripts` 报告新增 109 个包、审计 110 个包、0 个报告漏洞。这仅是安装当时 npm 的审计结果，不代表独立安全审计或未来依赖没有风险；没有启用依赖安装脚本。

初次生产构建的主 JavaScript 为 584.56 kB，gzip 为 185.11 kB。构建保留超过 500 kB 的 chunk 警告，未通过增大告警阈值或关闭检查隐藏问题。该数值仅对应当次构建，后续源码修补未以此数值重新计量。

## 实际命令与结果

以下命令由主协调或相应实现任务实际执行并报告退出码。不同时间的检查分别记录，不将后续测试倒计入先前浏览器所用 Binary。

| 工作目录与命令 | 实际结果 |
|---|---|
| `crates/kenvoy/frontend`：`npm install --ignore-scripts` | 退出 0；新增 109 包，审计 110 包，当时报告 0 漏洞 |
| 同目录：`npm run check` | 初次 6 项 TypeScript 错误；MUI 9 的 sx 类型及列表 descending 接口修正后退出 0 |
| 同目录：`npm run build` | 退出 0；真实 Vite/Rolldown React 资产生成，保留大 chunk 警告 |
| 根目录：`cargo test -p kenvoy --bin kenvoy frontend::tests --offline --locked` | 退出 0，1/1；内嵌资产可复用，篡改缓存时拒绝覆盖 |
| 根目录：`cargo build -p kenvoy --offline --locked` | 退出 0；形成携带真实前端资产的 Binary |
| 根目录：`node crates/kenvoy/frontend/scripts/forms-e2e.mjs` | 首次因测试期望文案与实际文案不符而失败；修正脚本后退出 0，不是产品功能修复 |
| `crates/kenvoy/frontend`：`node --test tests/*.test.mjs` | 后续最后一次退出 0；83/83：68 个共用字段向量、4 项额外校验、11 项 API 客户端测试 |
| 同目录：`npm run check` | API 超时、响应形状和 Session 并发修补后再次退出 0 |
| 根目录：`node tools/verify-docs.mjs` | 本次文档检查退出 0；54 个 Markdown、203 条本地链接通过 |

Rust Workspace 全量检查本阶段尚未重跑。阶段 05 的 168 项结果只属于[历史记录](0007-http-auth.md)，不能作为阶段 06 最终源码全量通过的证据。

## 首次浏览器通过的行为

真实浏览器打开 `/customers`，显示客户姓名、电话、年龄和提交操作；空姓名、超过 100 字符、非整数年龄和负年龄得到可见错误。填写“张三 / 13800000000 / 25”后观察到真实 POST 201，响应为真实 Resource Runtime 的记录，页面显示成功和记录编号。

匿名列表请求返回 401。随后在真实登录页面输入临时测试管理员凭据，登录成功后通过同一浏览器上下文的认证 API 获取列表，记录 ID 与刚提交的 ID 一致。此步骤验证 UI 登录与真实数据库读写衔接，不等于已经验收完整后台管理界面。

桌面视口为 1440×1000，移动视口为 390×844；移动端检查未发现横向溢出，浏览器 `pageerror` 列表为空。保存并由主协调和 UI 实现任务查看的截图为：

- [登记页桌面图](../../artifacts/phase06/form-desktop.png)
- [登记成功图](../../artifacts/phase06/form-success.png)
- [登记页移动图](../../artifacts/phase06/form-mobile.png)

测试仅使用独占临时项目与账号；源码中的测试口令不能作为正式默认管理员密码。脚本关闭测试服务，不将此次流程描述为 Ctrl+C、浏览器跨重启持久化或生产部署验收。

## 后续定向校验与修正

`validation.test.mjs` 从 Rust 共用的 `crates/kenvoy/tests/fixtures/validation.json` 直接读取全部 68 个向量，验证 Unicode White_Space、标量字符数、NUL、有限数与安全整数、严格日期、显式时区、UTC 毫秒规范化、select 和可空语义。另验证默认值与 Patch 缺失、无效 JavaScript 值、孤立 UTF-16 代理项、DOM 草稿转换，以及原型字段不污染输出。

客户端检查期间修复 JavaScript 正则 `$` 可在末尾换行前匹配的问题，严格拒绝多余日期/时间字符。整数草稿先检查十进制原文，避免很大的小数在 Number 舍入后被误认为整数。此额外 DOM 输入检查不改变 JSON 字段验证契约。

API 测试使用受控 Fetch 替身，验证协议和客户端状态，不将其称为真实 HTTP E2E：所有写入只发送一次；网络中断、超时或成功响应损坏标为结果待核对；字段错误与 request_id 保留；已知未执行的过载单独处理。30 秒期限覆盖请求与响应体读取，使用 AbortController 释放真实请求。内存保存 CSRF 与代际，不使用 LocalStorage 保存认证状态，不读写 HttpOnly Cookie。

刷新 Session 不悄悄更新旧页面的应用代际。Session GET 与身份变更串行，防止旧 GET 的 Set-Cookie 覆盖新登录；同实例并发登录/退出在发送前明确拒绝。旧请求的 401 不撤销较新的登录。中文表单标识使用 encodeURIComponent 放入 Header，HTTP 严格解码一次；服务端独立回归见前阶段记录中的后续联调条目。

上述 83 项最终客户端测试及并发/超时修补发生在首次浏览器构建验证之后。因此本阶段记录不能声称“同一最终构建同时通过 83 项测试和全部浏览器流程”；后续阶段需要重新构建 Binary 并复验浏览器。

## 限制与后续验证

后续优先完成阶段 07 后台真实列表、搜索、分页、排序、详情、增改删、不同角色边界及重启持久化，再以最新前端源码重新执行登记流程。独立 UI 审查还发现编辑草稿可能把未修改的 null 转为空串，已转阶段 07 以 dirty 字段和 Patch 缺失处理，并要求浏览器回归；不作为本阶段已验证行为。

未执行阶段 06 全量 Rust 门禁、全字段真实控件矩阵、性能预算基准、离线/弱网设备覆盖或正式 HTTPS 部署。未改变双 Crate 边界，未修改用户业务库，未提交、推送或对外发布。文件清单见[阶段 06 变更](../changes/0007-react-forms.md)。
