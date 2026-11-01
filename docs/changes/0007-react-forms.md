# 通用 React 表单 Runtime 与内置发行资产

状态：已执行；日期：2026-11-01；前置：[HTTP 与认证](0006-http-auth.md)。

## 背景与方案

阶段 05 已具备真实资源 API、认证与浏览器描述。本阶段由通用 React Runtime 解释这些描述，提供客户登记表单和登录界面，并将框架前端资产嵌入唯一 Rust Binary，使基础 DSL 项目启动无需另行启动前端服务或安装 npm 依赖。

页面与资源不生成独立业务组件源码；KEngine 不参与 React、HTTP 或业务验证。浏览器类型、校验和请求使用与服务端相同的投影协议，字段测试直接共用 Rust JSON 向量。UI 采用既定 React/MUI/CSR 路线，未更换技术栈。

## 实施文件

- 新增 `crates/kenvoy/frontend/package.json`、`package-lock.json`、`tsconfig.json`、`vite.config.mjs`、`index.html`：锁定前端依赖与 TypeScript/Vite 构建入口。
- 新增 `frontend/src/main.tsx`、`App.tsx`、`Runtime.tsx`、`theme.ts`、`styles.css`、`env.d.ts`：通用布局、页面树、字段/表单、登录、反馈与组件注册接口。后台、扩展组件及开发更新继续按后续阶段验收。
- 新增 `frontend/src/types.ts`、`validation.ts`、`api.ts`：完整公开 DTO、七类字段约束、默认值/Patch 规则、DOM 转换、同源请求、CSRF/代际、错误与不确定写入结果、请求期限和 Session 并发保护。
- 新增 `frontend/tests/validation.test.mjs`、`api.test.mjs`：共用字段向量和客户端协议定向测试；新增 `frontend/scripts/forms-e2e.mjs`：真实浏览器登记/登录/认证读取流程。
- 新增 `crates/kenvoy/build.rs`、`src/frontend.rs`，修改宿主接入：编译前要求真实前端 dist，内嵌公开 index/assets；运行时按内容指纹写入项目 `.kenvoy/runtime`，复用并校验缓存，篡改时保留现场并报错。
- HTTP 联调补充中文路径/表单标识严格解码，避免 Unicode form ID 直接进入 Fetch Header；共用测试与兼容性由相应 HTTP 定向测试覆盖。
- 更新前端现状文档、阶段分类索引与总索引，新增本记录及[独立验证报告](../verification/0008-react-forms.md)。保存阶段 06 三张真实浏览器截图于 `artifacts/phase06/`。

没有删除用户代码、改变 Git 历史、创建第三个 Rust Crate、建立独立后端进程或迁移用户业务数据库。当前包版本仍为 0.1.0，没有执行发布。

## 接口与边界

`ApiClient` 提供 getSession、getApplication、login/logout、list/get/create/update/delete。Cookie 仅由浏览器管理，CSRF 与应用代际仅驻留内存。`ApiError` 区分状态码、错误码、字段错误、request_id 和 outcomeUnknown；写入没有自动重试。默认请求期限 30 秒，覆盖 Fetch 与响应体；旧应用描述继续使用原代际，直到明确刷新描述。

`validateField` 与 `validateRecord` 返回结构化成功值或错误列表；Create 处理默认值和 null，Patch 只处理显式字段。`normalizeInput` 仅负责显式 DOM 草稿转换，不替代字段验证。文本不自动 trim，datetime 不猜测本地时区，统一为严格 UTC 毫秒。普通 React 扩展仅预留注册接口，本阶段没有宣称自定义模块构建或 Server Function 已实现。

基础应用使用内嵌资产；开发框架源码时须先在 frontend 安装锁定依赖并构建 dist，然后编译 Rust。缺少发行资产明确报错，不回退为静态假页面；启动不触发隐式安装。前端缓存与 SQLite 持久化位置分离。

## 执行验证

环境为 Windows/PowerShell、Node.js 24.22.0、Rust 1.99.1、Playwright 1.64.1 与 Chromium 156。`npm install --ignore-scripts` 退出 0，新增 109 包、审计 110 包，当时报告 0 漏洞；锁文件记录确切来源和完整性。TypeScript 初次六项错误经接口适配后解决，`npm run check` 退出 0。

首次 `npm run build` 退出 0，JavaScript 584.56 kB、gzip 185.11 kB，保留大于 500 kB 的 chunk 警告。内置资产 Rust 定向测试 1/1 和 Binary 编译退出 0。真实 `forms-e2e.mjs` 在修正测试文案期望后退出 0，验证必填、maxLength、整数/min、张三真实 201/SQLite 回显、匿名列表拒绝、管理员 UI 登录后认证读取同记录、390 像素无横向溢出，并保存查看三张截图。

此后客户端安全修补再次通过 `npm run check` 和 `node --test tests/*.test.mjs`，最后 83/83 通过，包含 68 个 Rust 共用向量、4 项额外校验与 11 项 API 协议测试。该次客户端测试不与早先浏览器构建混为同一最终门禁；阶段 07 要求重新构建并复验。阶段 06 未重跑 Rust 全量测试，阶段 05 的 168 项仅作为历史证据。

本阶段尚未验收完整后台 CRUD 界面、最新修补构建的完整浏览器流程、生产 run、HMR、自定义 UI 或服务端函数。具体命令、失败原因、验证时间边界与后续建议保留于[阶段 06 验证](../verification/0008-react-forms.md)。
