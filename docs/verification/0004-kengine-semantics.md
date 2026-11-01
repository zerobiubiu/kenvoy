# 阶段 02：通用语义、表达式与增量分析验收

状态：已执行；环境：Windows x86_64-pc-windows-msvc，Rust 1.99.1，2026-11-01。

## 实施范围

KEngine 新增结构化表达式、逐变量初始化与解构模式、词法作用域、值/类型符号、引用、ESM 跨模块绑定和调用方拥有的增量会话。仍是独立 Library，不包含业务模型或宿主执行能力。

增量测试使用真实临时文件，每步将结果与 Engine 全量分析比较，并验证 parsed/reused/evicted/affected。覆盖未变源码、内容变化、无效语法与恢复、删除重建、入口变化、解析选项、resolver 选项、扩展名优先级、原子替换、循环、无效 UTF-8 和根目录错误。错误候选不覆盖最后有效语言快照。

跨模块测试覆盖命名/default/namespace 导入，显式和星号重导出、循环固定点、歧义、显式覆盖、类型专用视图及非法 UTF-16 名称。JSX 属性保留动态引用；默认导出的 JSX 与函数体 JSX 明确区分。

## 发现并修复

- 独立回归首次发现 `_Panel`/`$Panel` 引用遗漏；移除 React 大小写假设后通过。
- 参数默认值不能看见函数体内声明；拆分参数环境与函数体作用域，覆盖大写、小写、下划线和美元符号 JSX 名称。
- `import type` 引用 const/function 可用于 `typeof`，新增类型查询视图且不开放运行期值导出。
- 显式转导指向歧义的星号导出原先漏诊断；新增 KE2102 回归。不可无损转换的名称使用 KE2103，不回退为 default。
- 并行接入时出现重复模块声明，已合并；Clippy 条件折叠与格式差异修复后重跑，无关闭检查。

## 实际验证

本轮最终稳定工具链检查前设置进程内 CARGO_INCREMENTAL=0；没有修改全局 Cargo 配置。

| 命令 | 结果 |
|---|---|
| cargo check --workspace --offline --locked | 退出 0 |
| cargo check -p kengine --offline --locked | 退出 0 |
| cargo test --workspace --offline --locked | 退出 0；81 项常规测试通过，1 项浏览器探针按原配置忽略 |
| cargo clippy --workspace --all-targets --offline --locked -- -D warnings | 退出 0 |
| cargo fmt --all -- --check | 首轮有格式差异；统一格式化后退出 0 |
| node tools/verify-architecture.mjs | 退出 0；2 个自有 Crate，KEngine 普通依赖闭包 103 包，无业务宿主依赖 |
| node tools/verify-docs.mjs | 执行时 33 个 Markdown、116 条本地链接通过；后续文档增加另行复验 |

最终常规测试构成：KEngine 66 项（18 API、7 表达式、8 增量、4 独立转导回归、8 模块绑定、6 历史语言探针、12 语义、3 独立语义回归）；Kenvoy 15 项原有边界与技术探针。子进程内看门狗测试输出不重复计数。此次没有重新运行前端浏览器探针；此前结果保持其历史覆盖范围。

Rust 1.98.0 另在 target/msrv-1.98 通过当时的 64 项 KEngine 测试，退出 0；该次运行早于最后新增的 2 项 type-only 转导测试，不将后者冒充已在该次 MSRV 运行。

## 结论与边界

阶段 02 核心验收通过，进入 Kenvoy DSL 阶段。当前没有完整 TS 类型检查、namespace 成员属性求解、Node/npm 条件解析、并行解析或持久化缓存。增量缓存仍逐轮读取可达源码内容、重算模块图和链接；不重解析全部未变源码。

本阶段没有创建业务数据库、HTTP Runtime 或前端应用；81 项测试不表示客户管理应用已经完成。后续必须通过真实 DSL、迁移、API、认证、React 和 CLI/E2E 验收。
