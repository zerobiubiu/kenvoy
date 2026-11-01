# 0002：Axum 托管 React 资产与真实浏览器同源验证

状态：已执行；前置：[阶段 00 技术探针](0001-stage00-probes.md)；日期：2026-11-01。

## 补充目标

P03 证明 Axum 同端口 HTTP/SQLite 与静态 shell，P04 原来用 Node 测试静态服务。本次把真实 Vite 构建的 React/MUI 产物交由 Axum 托管，用 Chromium 同源请求 SQLite API，确认两条技术链可以在同一 Rust 宿主协作。

本次仍是 test-local 探针。页面表单只做本地交互；写数据库的是浏览器脚本独立 fetch，不能宣称正式 Form Runtime 已连 API。身份门控不是生产 Session/CSRF。

## 实施与边界

修改 p03_http_sqlite.rs，抽出共同的 server 启动/关闭，增加 ServeFile/ServeDir 托管现有 React 产物及一个明确 ignored 的跨工具测试。普通 cargo test 不依赖 Node、npm 或浏览器；该测试须通过 --ignored 明确运行，不能把 ignored 计为已执行。

新增 tools/probes/frontend/scripts/axum-browser.mjs，限制传入 loopback 根地址，finally 关闭浏览器。Rust 启动临时 SQLite 与 Axum，运行脚本，回收 Node/服务器/连接，并重新打开文件检查真实记录和清理临时目录。Node 进程有 60 秒总期限，超时会终止并等待该进程。

## 执行验证记录

Windows/PowerShell，Rust 1.99.1，Node 24.22.0，Chromium 156.0.8078.4，bundled SQLite 3.54.1。脚本使用现有锁定依赖和浏览器，没有额外安装。

1. node --check scripts/axum-browser.mjs：退出 0。
2. 初次带 --locked 的 Cargo 测试退出 1，原因是阶段 01 正在把依赖从 dev 提升到正式层并加入本地路径依赖，需要更新锁文件；没有业务测试执行。
3. cargo generate-lockfile --offline：退出 0，仍为 203 个外部包，未升级精确直接版本。
4. cargo test -p kenvoy --test p03_http_sqlite axum_serves_built_react_and_sqlite_to_real_chromium --locked -- --ignored --nocapture：退出 0，1 passed / 0 failed，2.73s。

实际输出关键摘录：

```json
{
  "probe": "P03-P04-axum-browser",
  "status": "passed",
  "browser": "156.0.8078.4",
  "page": "/customers",
  "muiFields": 3,
  "hookCounter": 2,
  "created": {"status": 201, "record": {"age": 25, "id": 1, "name": "Axum浏览器"}},
  "anonymousList": 401,
  "unknownResource": 404,
  "pageErrors": 0
}
```

Rust 复查：端口已释放，重开 SQLite 有 1 条记录，临时目录已清理。结论是同端口技术集成通过；正式声明解释、权限、表单 API 绑定及完整 customer-app 闭环仍待相应阶段实现。
