# 0002：KEngine 正式解析基础与宿主公共 API 消费

状态：已执行；范围：阶段 01；前置：[阶段 00 探针](0001-stage00-probes.md)。

## 背景与方案

最小探针已经证明默认技术路线可协作，正式库仍为空。将成熟 Parser 隔离在 KEngine 内部，以自有版本化结构向宿主提供语法、位置、模块图与诊断；不将 Web、Schema、数据库或 React 语义引入核心库。

## 实施与接口

- 修改 crates/kengine/src/lib.rs，新增 engine.rs/parser.rs/source.rs/ir.rs/module.rs/diagnostics.rs。
- 新增 crates/kengine/tests/engine_api.rs（18 个正式测试），新增 crates/kenvoy/tests/engine_boundary.rs（2 个消费测试）。
- 调整根及两包 Cargo.toml/Cargo.lock：KEngine 通用 SWC/serde/thiserror 升为正式依赖，Kenvoy 增加本地 kengine 依赖，版本仍为未发布的 0.1.0；没有新增自有 Crate。
- 补强 p02_javascript.rs/p03_http_sqlite.rs 的独立看门狗、超时回收和自有进程守卫，修正 p05_watcher.rs 事件期限；新增前端 axum-browser.mjs 供显式跨工具验证。
- 新增根 README、KEngine API 与 Generic IR 文档，更新架构/边界/技术栈/风险/路线、验证与变更索引。

重要接口为 Engine::new、parse_source、parse_file、analyze_project、resolve_module，结果自身提供诊断和图访问。Generic IR v1 保留源码、UTF-16 字面量、半开字节范围、Unicode 标量位置、JSX 出现和动态表达式引用；未知格式解码拒绝。完整 API 见 [说明](../contracts/kengine-api.md)，数据协议见 [Generic IR](../contracts/generic-ir.md)。

没有删除文件、改写历史、创建提交或发布。新增初始公共 API 尚未对外发布，不为内部阶段实现机械递增版本。正式 Kenvoy main 仍为空，不伪装成已有 check/dev/run 命令。

## 已知限制与后续建议

阶段 02 再建立完整作用域/符号/引用与增量分析；本阶段标识符只保存语法文本，动态表达式只保留结构/源码，不执行。完整 npm/tsconfig 条件解析、业务应用、迁移、认证和端到端 customer-app 仍待对应阶段。

## 执行验证记录

环境：Windows/PowerShell，主 Rust 1.99.1/MSVC；为核对声明的 MSRV，自主安装 Rust 1.98.0 minimal 后单独运行；默认 stable 未改。临时 CARGO_INCREMENTAL=0，MSRV 产物放独立 target/msrv-1.98。

- cargo check -p kengine --locked、cargo test -p kengine --locked：各退出 0，独立 KEngine 24 项通过。
- cargo +1.98.0 test -p kengine --offline --locked：退出 0，24 项通过。
- cargo check --workspace --locked、cargo test --workspace --locked：各退出 0，39 项常规测试通过，1 项跨工具明确 ignored。
- cargo clippy --workspace --all-targets --locked -- -D warnings、cargo fmt --all -- --check：各退出 0。
- 显式执行 p03_http_sqlite 的 axum_serves_built_react_and_sqlite_to_real_chromium（--ignored --nocapture）：退出 0，真实浏览器/同源 API/SQLite 重开检查通过。
- Cargo metadata 架构断言退出 0：成员数 2，目标类型和依赖方向符合要求，KEngine 普通依赖无业务技术栈。

失败与修复、实际统计及残余风险见 [阶段 01 报告](../verification/0003-kengine-foundation.md)。首轮大枚举 Clippy 与旧增量缓存 note 均保留记录，没有放宽检查。后续建议按路线完善通用语义并持续验证边界。
