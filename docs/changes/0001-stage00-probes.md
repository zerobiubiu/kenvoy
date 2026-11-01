# 0001：建立阶段 00 可复现技术探针

状态：已执行；范围：工程依赖、独立技术验证和项目授权同步。

## 背景与方案

此前只有空骨架，资料支持不足以证明所选技术可协作。用户授予本项目工程必要依赖的自主决策权后，锁定 Rust/npm 依赖，在现有两个包的 tests 及独立前端探针中验证技术路线，保留正式 src 为空，不把探针当产品能力。

## 实施

修改根 Cargo.toml：明确两个成员并集中锁定依赖；两个 Crate manifest 只增加探针 dev-dependencies；更新 Cargo.lock。修改 .gitignore 排除 node_modules、浏览器/构建结果和 .probe；修改 AGENTS.md 记录用户自主执行授权。

新增 kengine/tests/p01_language.rs 与 fixtures/p01；kenvoy/tests/p02_javascript.rs、fixtures/p02-javascript、p03_http_sqlite.rs、p05_watcher.rs；新增 tools/probes/frontend 的 package/lock、配置、组件、样式、描述、服务端隔离负例及浏览器脚本。完整文件范围可通过 git status 和上述目录发现；没有删除原有源码，没有新增第三个 Crate或发布公共 API。

P01 是有限转换；P02 是受限纯函数；P03 是固定 Schema/身份门控；P04 页面不连数据库；P05 是测试本地缓存模型。现状文档和索引同步区分这些边界，完整证据见 [验证报告](../verification/0001-stage00-probes.md)。

## 后续建议

进入阶段 01 实现正式 KEngine API 和通用模型，补全表达式、模块和诊断契约。数据库、认证、前端解释器和整体应用热更按路线分别实现，不能从固定测试代码直接宣称完成框架。

## 执行验证记录

环境：Windows/PowerShell，Rust 1.99.1 MSVC，Node 24.22.0/npm 12.2.0，Chromium 156.0.8078.4，SQLite 3.54.1；基线 main/b381cba。

- cargo fetch：退出 0，锁定 203 个外部包。
- cargo check --workspace --locked：退出 0。
- cargo test --workspace --locked：退出 0，18 个探针测试通过，0 失败。
- cargo clippy --workspace --all-targets --locked -- -D warnings：退出 0。
- cargo fmt --all -- --check：退出 0。
- tools/probes/frontend 中 npm.cmd ci --ignore-scripts --no-audit --no-fund、npm.cmd run build、修正后的 npm.cmd run probe：各退出 0，105 包、生产构建与 7 项浏览器检查通过。

初次失败和修复详见上述验证报告，不隐藏失败。结论：阶段 00 最小技术可行性通过；未验证正式业务 Runtime、MSRV 1.98、完整安全模型或任意 React 编辑形态。没有提交、推送或部署。
