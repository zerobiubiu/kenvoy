# CLI、迁移与发布操作文档

状态：参考；治理：现状文档。

返回[文档总索引](../README.md)。

## 目录职责

集中说明 Kenvoy 的本机 CLI、开发与生产生命周期、数据库迁移、产品发布及数据保护操作。这里记录现行操作行为和执行边界；配置与数据结构的契约归入 [contracts](../contracts/README.md)，实现机制归入 [architecture](../architecture/README.md)，不因文件名含有 design 就将已实现行为当作未来计划。

## 文档清单

| 文档 | 状态 | 内容概览 |
|---|---|---|
| [CLI 当前行为](cli-design.md) | 参考 | 项目创建、检查与解析结果、迁移、账号管理、dev/build/run、依赖安装、产物部署、退出和日志 |
| [迁移执行协议](migration-design.md) | 参考 | Schema Diff、只读预览、漂移检查、安全应用、checksum 确认、一致性备份、事务与恢复边界 |
| [Git、版本标签与产品发布](releasing.md) | 参考 | dev/main 双分支、自动本地 tag、main 当前版本自动发布及手动兜底、单目标下载 artifact、六目标 registry 构建与安装边界 |
| [多渠道软件包发布](distribution-channels.md) | 参考 | npm、Homebrew、Scoop、WinGet、Chocolatey、AUR、APT/RPM/Alpine/Arch Pages 仓库的产物流、Secret 与外部审核边界 |

## 推荐阅读顺序

1. 首次使用先读[项目 README](../../README.md)准备环境，再读[CLI 当前行为](cli-design.md)选择源码运行或完整产物运行方式。
2. 启动或部署前核对[项目配置契约](../contracts/project-configuration.md)，尤其是持久化路径、监听地址、对外 Origin 与运行信任设置。
3. 变更数据模型前先读[迁移执行协议](migration-design.md)，再按 CLI 文档进行只读预览；破坏性操作的确认与备份不能替代数据有效性检查。
4. 排查实现问题时进入[Runtime 设计](../architecture/runtime-design.md)、[数据库设计](../architecture/database-design.md)和[安全设计](../architecture/security-design.md)；复现验证的方法与工具前置条件见[工程流程](../development/engineering-workflow.md)。
5. 发布操作者阅读[发布操作](releasing.md)及[版本日志](../changes/CHANGELOG.md)，各次对外操作的事实与限制从编号变更记录进入。

## 关联契约、代码与配置

- 操作输入与模型契约：[项目配置](../contracts/project-configuration.md)、[DSL 语法](../contracts/dsl-specification.md)、[Application IR](../contracts/application-ir.md)。组件重载、联动条件与远程选项是否影响 Schema，结合[组件与选项契约](../contracts/component-bindings.md)及迁移正文判断。
- CLI 与项目加载：[main.rs](../../crates/kenvoy/src/main.rs)、[project.rs](../../crates/kenvoy/src/infrastructure/project/mod.rs)；产物与生命周期：[artifact.rs](../../crates/kenvoy/src/infrastructure/artifacts/mod.rs)、[lifecycle.rs](../../crates/kenvoy/src/bootstrap/lifecycle.rs)。
- 数据库执行与迁移：[database](../../crates/kenvoy/src/infrastructure/persistence/sqlite/)、[migration.rs](../../crates/kenvoy/src/infrastructure/persistence/sqlite/migration/mod.rs)；认证与 HTTP：[auth.rs](../../crates/kenvoy/src/application/identity/mod.rs)、[http](../../crates/kenvoy/src/interfaces/http/)。
- 可阅读的项目配置示例：[customer-app/kenvoy.toml](../../examples/customer-app/kenvoy.toml)；示例用途与入口见[示例说明](../../examples/README.md)。示例数据目录不是测试清理或恢复覆盖的目标。
- 当前操作回归与工程入口：[Kenvoy tests](../../crates/kenvoy/tests/)、[kenvoy-tests](../../crates/kenvoy-tests/)、[xtask](../../xtask/)。命令与测试所有权以[工程流程](../development/engineering-workflow.md)为准。

## 证据与操作边界

- 迁移的阶段证据见[阶段 04 SQLite 验证](../verification/0006-sqlite-runtime.md)，开发、构建、生产与真实 Ctrl+C 见[阶段 09 生命周期验证](../verification/0011-cli-lifecycle.md)，第一版综合范围见[阶段 10 最终验收](../verification/0012-final-acceptance.md)。
- 首版后组件与选项行为的专项证据见[变更 0012](../changes/0012-native-components-and-options.md)，工程与测试入口替代证据见[变更 0013](../changes/0013-rust-test-workspace-and-tools-retirement.md)，不将其混入早期阶段验收。历史测试计数不代表当前修订计数。
- 当前没有候选重命名的映射执行接口或自动恢复命令；确认不能绕过预检。磁盘满、断电与极大数据集等未覆盖条件见[技术风险](../development/technical-risks.md)。本入口不额外授予迁移、删除或覆盖用户数据的权限。

## 维护、废弃与替代关系

本目录三个主题均为可持续维护的现状参考，当前无废弃主题。CLI 参数、配置、默认行为、迁移、恢复或发布边界变化时，同步维护操作正文、相应契约与实现设计，并在[变更记录](../changes/README.md)保存当次事实；设计或待验证内容不得写作已验收能力。

定稿 ADR、变更和验证正文不合并、不删除、不重编号，后续变化或勘误另建记录并更新索引。原 tools 已退役，历史记录中的旧路径与命令仅保留历史事实；当前工程操作使用 Rust xtask，测试与进程夹具使用 kenvoy-tests，不恢复旧工具链作为现行依赖。
